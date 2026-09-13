use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Limit the specificity of selectors.
///
/// Accepts a specificity string like `"0,2,0"` as the primary option,
/// representing max (id, class, type). Defaults to `"0,2,0"` if no option
/// is provided.
///
/// Equivalent to Stylelint's `selector-max-specificity` rule.
///
/// ## Options
///
/// Primary: `"<id>,<class>,<type>"` — maximum allowed specificity.
///
/// Secondary:
/// - `ignoreSelectors: [":is", ":has", "/my-/", ...]` — pseudo-classes/selectors
///   matching these strings or regex patterns have their specificity contribution
///   ignored.
pub struct SelectorMaxSpecificity;

const DEFAULT_MAX_ID: usize = 0;
const DEFAULT_MAX_CLASS: usize = 2;
const DEFAULT_MAX_TYPE: usize = 0;

/// Parse a specificity string like `"0,2,0"` into `(id, class, type)`.
fn parse_specificity_option(s: &str) -> Option<(usize, usize, usize)> {
  let parts: Vec<&str> = s.split(',').collect();
  if parts.len() != 3 {
    return None;
  }
  let id = parts[0].trim().parse::<usize>().ok()?;
  let class = parts[1].trim().parse::<usize>().ok()?;
  let typ = parts[2].trim().parse::<usize>().ok()?;
  Some((id, class, typ))
}

/// Configuration parsed from rule options.
struct Config {
  max_id: usize,
  max_class: usize,
  max_type: usize,
  ignore_selectors: Vec<IgnorePattern>,
}

enum IgnorePattern {
  Exact(String),
  Regex(String),
}

impl Config {
  fn from_context(ctx: &RuleContext) -> Self {
    let (max_id, max_class, max_type) = ctx
      .primary_option()
      .and_then(|v| v.as_str())
      .and_then(parse_specificity_option)
      .unwrap_or((DEFAULT_MAX_ID, DEFAULT_MAX_CLASS, DEFAULT_MAX_TYPE));

    let secondary = ctx.secondary_options();

    let ignore_selectors: Vec<IgnorePattern> = secondary
      .and_then(|v| v.get("ignoreSelectors"))
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .filter_map(|v| v.as_str())
          .map(|s| {
            if s.starts_with('/') && s.ends_with('/') && s.len() > 2 {
              IgnorePattern::Regex(s[1..s.len() - 1].to_string())
            } else {
              IgnorePattern::Exact(s.to_string())
            }
          })
          .collect()
      })
      .unwrap_or_default();

    Config {
      max_id,
      max_class,
      max_type,
      ignore_selectors,
    }
  }

  fn is_pseudo_ignored(&self, pseudo_name: &str) -> bool {
    // Check against patterns like ":is", ":has", "/my-/"
    for pat in &self.ignore_selectors {
      match pat {
        IgnorePattern::Exact(s) => {
          // Match ":is" against pseudo_name "is" (strip leading colon)
          let s_stripped = s.strip_prefix(':').unwrap_or(s);
          if s_stripped.eq_ignore_ascii_case(pseudo_name) {
            return true;
          }
        }
        IgnorePattern::Regex(re) => {
          // Simple regex: prefix match with ^
          let to_check = format!(":{pseudo_name}");
          if let Some(prefix) = re.strip_prefix('^')
            && (to_check.starts_with(prefix) || pseudo_name.starts_with(prefix))
          {
            return true;
          }
          // Simple contains check for non-anchored patterns
          if to_check.contains(re.as_str()) || pseudo_name.contains(re.as_str()) {
            return true;
          }
        }
      }
    }
    false
  }

  fn exceeds_max(&self, ids: usize, classes: usize, types: usize) -> bool {
    // Specificity comparison: compare component by component from left to right
    // A specificity of (1,0,0) exceeds (0,255,255) because id > max_id
    // But (0,3,0) vs max (0,2,1): 3 > 2 so it exceeds.
    // Actually Stylelint compares as a tuple: exceeds if any component is higher
    // when higher components are equal or lower. In practice, Stylelint compares
    // the specificity as a whole: (a,b,c) > (ma,mb,mc) iff
    // a > ma || (a == ma && b > mb) || (a == ma && b == mb && c > mc)
    if ids > self.max_id {
      return true;
    }
    if ids == self.max_id && classes > self.max_class {
      return true;
    }
    if ids == self.max_id && classes == self.max_class && types > self.max_type {
      return true;
    }
    false
  }
}

impl Rule for SelectorMaxSpecificity {
  fn name(&self) -> &'static str {
    "selector-max-specificity"
  }

  fn description(&self) -> &'static str {
    "Limit the specificity of selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let config = Config::from_context(ctx);

    let selector = if matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass | gale_css_parser::Syntax::Less
    ) {
      strip_preprocessor_interpolation(&rule.selector, ctx.syntax)
    } else {
      rule.selector.clone()
    };

    let mut diags = Vec::new();

    // Check each comma-separated selector individually
    for sel in selector.split(',') {
      let sel = sel.trim();
      if sel.is_empty() {
        continue;
      }

      // Skip selectors that look like preprocessor constructs
      if sel.starts_with('%') || sel.starts_with('$') || sel.contains("#{") || sel.contains("@{") {
        continue;
      }

      let (ids, classes, types) = compute_single_specificity(sel, &config);

      if config.exceeds_max(ids, classes, types) {
        diags.push(
                    Diagnostic::new(
                        self.name(),
                        format!(
                            "Expected selector \"{sel}\" to have a specificity no more than \"{},{},{}\", but got \"{ids},{classes},{types}\"",
                            config.max_id, config.max_class, config.max_type,
                        ),
                    )
                    .severity(self.default_severity())
                    .span(Span::new(rule.span.offset, rule.span.length)),
                );
      }
    }
    diags
  }
}

/// Extract the content inside balanced parentheses from a byte string,
/// advancing `i` past the closing `)`.  Assumes `selector.as_bytes()[*i] == b'('`.
fn extract_paren_content<'a>(selector: &'a str, i: &mut usize) -> &'a str {
  let bytes = selector.as_bytes();
  let mut depth = 1;
  *i += 1; // skip opening '('
  let start = *i;
  while *i < bytes.len() && depth > 0 {
    if bytes[*i] == b'(' {
      depth += 1;
    } else if bytes[*i] == b')' {
      depth -= 1;
    }
    if depth > 0 {
      *i += 1;
    }
  }
  let end = *i;
  if *i < bytes.len() {
    *i += 1; // skip closing ')'
  }
  &selector[start..end]
}

fn compute_single_specificity(selector: &str, config: &Config) -> (usize, usize, usize) {
  let mut ids = 0;
  let mut classes = 0;
  let mut types = 0;
  let bytes = selector.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    match bytes[i] {
      b'#' => {
        // Check for SCSS interpolation #{...}
        if i + 1 < len && bytes[i + 1] == b'{' {
          // Skip #{...}
          let mut depth = 1;
          i += 2;
          while i < len && depth > 0 {
            if bytes[i] == b'{' {
              depth += 1;
            } else if bytes[i] == b'}' {
              depth -= 1;
            }
            i += 1;
          }
        } else {
          ids += 1;
          i += 1;
          while i < len && is_ident_char(bytes[i]) {
            i += 1;
          }
        }
      }
      b'.' => {
        classes += 1;
        i += 1;
        while i < len && is_ident_char(bytes[i]) {
          i += 1;
        }
      }
      b'[' => {
        // Attribute selector counts as class-level
        classes += 1;
        i += 1;
        while i < len && bytes[i] != b']' {
          i += 1;
        }
        if i < len {
          i += 1;
        }
      }
      b':' => {
        i += 1;
        if i < len && bytes[i] == b':' {
          // Pseudo-element: counts as type
          types += 1;
          i += 1;
          while i < len && is_ident_char(bytes[i]) {
            i += 1;
          }
          // Skip parenthetical content
          if i < len && bytes[i] == b'(' {
            let mut depth = 1;
            i += 1;
            while i < len && depth > 0 {
              if bytes[i] == b'(' {
                depth += 1;
              } else if bytes[i] == b')' {
                depth -= 1;
              }
              i += 1;
            }
          }
        } else {
          // Pseudo-class -- read the name first
          let name_start = i;
          while i < len && is_ident_char(bytes[i]) {
            i += 1;
          }
          let name = &selector[name_start..i];

          if name.eq_ignore_ascii_case("where") {
            // :where() has zero specificity -- skip arguments
            if i < len && bytes[i] == b'(' {
              let mut depth = 1;
              i += 1;
              while i < len && depth > 0 {
                if bytes[i] == b'(' {
                  depth += 1;
                } else if bytes[i] == b')' {
                  depth -= 1;
                }
                i += 1;
              }
            }
          } else if name.eq_ignore_ascii_case("host") {
            // :host has specificity (0,0,0) as a pseudo-class itself,
            // but :host(.selector) adds the argument's specificity
            // See: https://drafts.csswg.org/css-scoping/#host-selector
            if i < len && bytes[i] == b'(' {
              if config.is_pseudo_ignored(name) {
                // Skip the argument
                let mut depth = 1;
                i += 1;
                while i < len && depth > 0 {
                  if bytes[i] == b'(' {
                    depth += 1;
                  } else if bytes[i] == b')' {
                    depth -= 1;
                  }
                  i += 1;
                }
              } else {
                let inner = extract_paren_content(selector, &mut i);
                let mut max_i = 0;
                let mut max_c = 0;
                let mut max_t = 0;
                for arg in inner.split(',') {
                  let (ai, ac, at) = compute_single_specificity(arg.trim(), config);
                  max_i = max_i.max(ai);
                  max_c = max_c.max(ac);
                  max_t = max_t.max(at);
                }
                ids += max_i;
                classes += max_c;
                types += max_t;
              }
            }
            // :host without parens = (0,0,0) specificity-wise
            // but it does contribute as a pseudo-class
            classes += 1;
          } else if name.eq_ignore_ascii_case("is")
            || name.eq_ignore_ascii_case("not")
            || name.eq_ignore_ascii_case("has")
            || name.eq_ignore_ascii_case("matches")
          {
            // :is(), :not(), :has(), :matches() take the specificity of the
            // most specific argument.
            if i < len && bytes[i] == b'(' {
              if config.is_pseudo_ignored(name) {
                // Skip the argument entirely
                let mut depth = 1;
                i += 1;
                while i < len && depth > 0 {
                  if bytes[i] == b'(' {
                    depth += 1;
                  } else if bytes[i] == b')' {
                    depth -= 1;
                  }
                  i += 1;
                }
              } else {
                let inner = extract_paren_content(selector, &mut i);
                let mut max_i = 0;
                let mut max_c = 0;
                let mut max_t = 0;
                for arg in inner.split(',') {
                  let (ai, ac, at) = compute_single_specificity(arg.trim(), config);
                  max_i = max_i.max(ai);
                  max_c = max_c.max(ac);
                  max_t = max_t.max(at);
                }
                ids += max_i;
                classes += max_c;
                types += max_t;
              }
            }
          } else if name.eq_ignore_ascii_case("nth-child")
            || name.eq_ignore_ascii_case("nth-last-child")
          {
            // :nth-child() counts as a pseudo-class (specificity class-level)
            classes += 1;
            // If there's an "of" clause like :nth-child(2n of .foo),
            // add the specificity of the most specific selector in the of-clause
            if i < len && bytes[i] == b'(' {
              let inner = extract_paren_content(selector, &mut i);
              // Check for "of" keyword
              if let Some(of_pos) = find_of_clause(inner) {
                let of_selectors = &inner[of_pos..];
                let mut max_i = 0;
                let mut max_c = 0;
                let mut max_t = 0;
                for arg in of_selectors.split(',') {
                  let (ai, ac, at) = compute_single_specificity(arg.trim(), config);
                  max_i = max_i.max(ai);
                  max_c = max_c.max(ac);
                  max_t = max_t.max(at);
                }
                ids += max_i;
                classes += max_c;
                types += max_t;
              }
            }
          } else {
            classes += 1;
            // Skip parenthetical content
            if i < len && bytes[i] == b'(' {
              let mut depth = 1;
              i += 1;
              while i < len && depth > 0 {
                if bytes[i] == b'(' {
                  depth += 1;
                } else if bytes[i] == b')' {
                  depth -= 1;
                }
                i += 1;
              }
            }
          }
        }
      }
      b'&' => {
        // Nesting selector — skip
        i += 1;
        while i < len && is_ident_char(bytes[i]) {
          i += 1;
        }
      }
      b' ' | b'>' | b'+' | b'~' | b'*' | b'\n' | b'\r' | b'\t' => {
        i += 1;
      }
      _ if is_ident_start(bytes[i]) => {
        types += 1;
        while i < len && is_ident_char(bytes[i]) {
          i += 1;
        }
      }
      _ => {
        i += 1;
      }
    }
  }

  (ids, classes, types)
}

/// Find the "of" clause in a :nth-child() argument, returning the byte
/// offset right after "of " where selectors start. Returns None if no
/// "of" clause is found.
fn find_of_clause(inner: &str) -> Option<usize> {
  // Look for " of " (case insensitive)
  let lower = inner.to_ascii_lowercase();
  lower.find(" of ").map(|pos| pos + 4)
}

/// Strip SCSS/Less interpolation from a selector string.
fn strip_preprocessor_interpolation(selector: &str, syntax: gale_css_parser::Syntax) -> String {
  let mut result = String::with_capacity(selector.len());
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    // SCSS: #{...}
    if matches!(
      syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass
    ) && i + 1 < len
      && chars[i] == '#'
      && chars[i + 1] == '{'
    {
      let mut depth = 1;
      i += 2;
      while i < len && depth > 0 {
        if chars[i] == '{' {
          depth += 1;
        } else if chars[i] == '}' {
          depth -= 1;
        }
        i += 1;
      }
    }
    // Less: @{...}
    else if matches!(syntax, gale_css_parser::Syntax::Less)
      && i + 1 < len
      && chars[i] == '@'
      && chars[i + 1] == '{'
    {
      let mut depth = 1;
      i += 2;
      while i < len && depth > 0 {
        if chars[i] == '{' {
          depth += 1;
        } else if chars[i] == '}' {
          depth -= 1;
        }
        i += 1;
      }
    } else {
      result.push(chars[i]);
      i += 1;
    }
  }
  result
}

fn is_ident_start(b: u8) -> bool {
  b.is_ascii_alphabetic() || b == b'_' || b == b'-' || b > 127
}

fn is_ident_char(b: u8) -> bool {
  b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b > 127
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn ctx_with_options(opts: &serde_json::Value) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(opts),
    }
  }

  fn style_with_selector(sel: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: vec![],
      span: ParserSpan::new(0, sel.len()),
      ..Default::default()
    })
  }

  #[test]
  fn reports_high_specificity() {
    // #id has specificity 1,0,0 which exceeds default 0,2,0
    let d = SelectorMaxSpecificity.check(&style_with_selector("#id"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("1,0,0"));
  }

  #[test]
  fn allows_low_specificity() {
    // .a .b has specificity 0,2,0 which is within default 0,2,0
    let d = SelectorMaxSpecificity.check(&style_with_selector(".a .b"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_too_many_classes() {
    // .a.b.c has specificity 0,3,0 which exceeds default 0,2,0
    let d = SelectorMaxSpecificity.check(&style_with_selector(".a.b.c"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("0,3,0"));
  }

  #[test]
  fn where_has_zero_specificity() {
    // :where(#id) should have (0,0,0), well under the limit
    let d = SelectorMaxSpecificity.check(&style_with_selector(":where(#id)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn is_takes_max_argument_specificity() {
    // :is(#id) should have (1,0,0) which exceeds limit
    let d = SelectorMaxSpecificity.check(&style_with_selector(":is(#id)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("1,0,0"));

    // :is(.a) should have (0,1,0) which is within default 0,2,0
    let d = SelectorMaxSpecificity.check(&style_with_selector(":is(.a)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn not_takes_max_argument_specificity() {
    // :not(#id) should have (1,0,0) which exceeds limit
    let d = SelectorMaxSpecificity.check(&style_with_selector(":not(#id)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("1,0,0"));
  }

  #[test]
  fn custom_option_allows_higher_specificity() {
    let opts = serde_json::json!(["0,4,1"]);
    let c = ctx_with_options(&opts);
    // .a.b.c.d has specificity 0,4,0 -- within "0,4,1"
    let d = SelectorMaxSpecificity.check(&style_with_selector(".a.b.c.d"), &c);
    assert!(d.is_empty());
  }

  #[test]
  fn custom_option_restricts_specificity() {
    let opts = serde_json::json!(["0,1,0"]);
    let c = ctx_with_options(&opts);
    // .a.b has specificity 0,2,0 -- exceeds "0,1,0"
    let d = SelectorMaxSpecificity.check(&style_with_selector(".a.b"), &c);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("0,1,0"));
  }

  #[test]
  fn tuple_comparison_type_allowed_when_class_within() {
    // With max "0,2,1": "span a" has specificity 0,0,2.
    // Since id=0<=0, class=0<=2, type=2>1 => exceeds.
    // Wait no: 0,0,2 vs 0,2,1: ids equal, classes 0<=2, types 2>1 => exceeds.
    // But Stylelint allows "span a" with max "0,2,0"... checking.
    // Actually Stylelint compares as tuple: (0,0,2) vs (0,2,0)
    // id: 0<=0, class: 0<=2 (less), so no exceed.
    // Only exceeds if a higher bucket is equal and a lower bucket exceeds.
    let opts = serde_json::json!(["0,2,0"]);
    let c = ctx_with_options(&opts);
    let d = SelectorMaxSpecificity.check(&style_with_selector("span a"), &c);
    assert!(d.is_empty(), "span a should be allowed with max 0,2,0");
  }

  #[test]
  fn parse_specificity_option_valid() {
    assert_eq!(parse_specificity_option("0,2,0"), Some((0, 2, 0)));
    assert_eq!(parse_specificity_option("1, 3, 2"), Some((1, 3, 2)));
  }

  #[test]
  fn parse_specificity_option_invalid() {
    assert_eq!(parse_specificity_option("abc"), None);
    assert_eq!(parse_specificity_option("0,2"), None);
    assert_eq!(parse_specificity_option("0,2,0,1"), None);
  }
}
