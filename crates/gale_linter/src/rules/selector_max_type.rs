use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Limit the number of type selectors in a selector.
///
/// Equivalent to Stylelint's `selector-max-type` rule.
/// Default maximum: 3. Counts bare element names (type selectors). Detection-only.
///
/// ## Options
///
/// Primary option: `<number>` — maximum allowed type selectors.
///
/// Secondary options:
/// - `ignore: ["descendant", "child", "compounded", "next-sibling", "custom-elements"]`
/// - `ignoreTypes: ["fieldset", "/^my-/"]` — type selectors matching these strings
///   or regex patterns are ignored.
pub struct SelectorMaxType;

const MAX_TYPE: usize = 3;

/// Configuration parsed from rule options.
struct Config {
  max: usize,
  ignore_descendant: bool,
  ignore_child: bool,
  ignore_compounded: bool,
  ignore_next_sibling: bool,
  ignore_custom_elements: bool,
  ignore_types: Vec<IgnorePattern>,
}

enum IgnorePattern {
  Exact(String),
  Regex(String), // stored without leading/trailing `/`
}

impl Config {
  fn from_context(ctx: &RuleContext) -> Self {
    let max = ctx
      .primary_option()
      .and_then(|v| v.as_u64())
      .map(|n| n as usize)
      .unwrap_or(MAX_TYPE);

    let secondary = ctx.secondary_options();

    let ignore_list: Vec<String> = secondary
      .and_then(|v| v.get("ignore"))
      .map(|v| {
        if let Some(arr) = v.as_array() {
          arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect()
        } else if let Some(s) = v.as_str() {
          vec![s.to_string()]
        } else {
          vec![]
        }
      })
      .unwrap_or_default();

    let ignore_types: Vec<IgnorePattern> = secondary
      .and_then(|v| v.get("ignoreTypes"))
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
      max,
      ignore_descendant: ignore_list.iter().any(|s| s == "descendant"),
      ignore_child: ignore_list.iter().any(|s| s == "child"),
      ignore_compounded: ignore_list.iter().any(|s| s == "compounded"),
      ignore_next_sibling: ignore_list.iter().any(|s| s == "next-sibling"),
      ignore_custom_elements: ignore_list.iter().any(|s| s == "custom-elements"),
      ignore_types,
    }
  }

  fn is_type_ignored(&self, type_name: &str) -> bool {
    for pat in &self.ignore_types {
      match pat {
        IgnorePattern::Exact(s) => {
          if s.eq_ignore_ascii_case(type_name) {
            return true;
          }
        }
        IgnorePattern::Regex(re) => {
          // Simple regex support: only `^prefix` patterns
          if let Some(prefix) = re.strip_prefix('^')
            && type_name.starts_with(prefix)
          {
            return true;
          }
        }
      }
    }
    false
  }
}

impl Rule for SelectorMaxType {
  fn name(&self) -> &'static str {
    "selector-max-type"
  }

  fn description(&self) -> &'static str {
    "Limit the number of type selectors in a selector"
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
      strip_preprocessor_constructs(&rule.selector, ctx.syntax)
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
      let count = count_type_selectors(sel, &config);
      if count > config.max {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!(
              "Expected \"{sel}\" to have no more than {max} type selector",
              max = config.max,
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

/// Represents a simple selector segment with its context.
struct SelectorSegment {
  name: String,
  is_after_descendant_combinator: bool,
  is_after_child_combinator: bool,
  is_after_next_sibling_combinator: bool,
  is_compounded: bool,
  is_custom_element: bool,
}

/// Count type selectors (bare element names like `div`, `span`, `a`),
/// respecting the `ignore` and `ignoreTypes` configuration.
fn count_type_selectors(selector: &str, config: &Config) -> usize {
  let segments = parse_selector_segments(selector);
  let mut count = 0;

  for seg in &segments {
    if !is_type_name(&seg.name) {
      continue;
    }

    // Check ignoreTypes
    if config.is_type_ignored(&seg.name) {
      continue;
    }

    // Check ignore options
    if config.ignore_custom_elements && seg.is_custom_element {
      continue;
    }
    if config.ignore_descendant && seg.is_after_descendant_combinator {
      continue;
    }
    if config.ignore_child && seg.is_after_child_combinator {
      continue;
    }
    if config.ignore_next_sibling && seg.is_after_next_sibling_combinator {
      continue;
    }
    if config.ignore_compounded && seg.is_compounded {
      continue;
    }

    count += 1;
  }
  count
}

/// Check if a name is a valid type selector (not a pseudo-class name, etc.)
fn is_type_name(name: &str) -> bool {
  if name.is_empty() {
    return false;
  }
  let first = name.as_bytes()[0];
  (first.is_ascii_alphabetic() || first > 127)
    && !name.eq_ignore_ascii_case("from")
    && !name.eq_ignore_ascii_case("to")
}

/// Parse a single (non-comma-separated) selector into segments.
/// Does NOT skip pseudo-class function arguments — type selectors inside
/// `:is()`, `:not()`, `:has()`, `:where()` are counted, matching Stylelint v17
/// as-written behavior.
fn parse_selector_segments(selector: &str) -> Vec<SelectorSegment> {
  let mut segments: Vec<SelectorSegment> = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  // Track the last combinator seen
  let mut last_combinator: Option<char> = None; // ' ', '>', '+', '~'
  let mut compound_has_other = false; // has other simple selectors in this compound
  // Track indices into `segments` for type selectors in the current compound
  let mut compound_type_indices: Vec<usize> = Vec::new();

  while i < len {
    // Skip whitespace — counts as descendant combinator
    if chars[i].is_ascii_whitespace() {
      // Finalize compound: mark type selectors as compounded if compound has other selectors
      if compound_has_other {
        for &idx in &compound_type_indices {
          segments[idx].is_compounded = true;
        }
      }
      compound_has_other = false;
      compound_type_indices.clear();

      if last_combinator.is_none() {
        last_combinator = Some(' ');
      }
      i += 1;
      continue;
    }

    match chars[i] {
      '>' | '+' | '~' => {
        // Finalize current compound before starting new one
        if compound_has_other {
          for &idx in &compound_type_indices {
            segments[idx].is_compounded = true;
          }
        }
        compound_has_other = false;
        compound_type_indices.clear();

        last_combinator = Some(chars[i]);
        i += 1;
      }
      '.' | '#' => {
        // Class or ID selector — marks compound as having other selectors
        compound_has_other = true;
        i += 1;
        while i < len && is_ident_char(chars[i]) {
          i += 1;
        }
        last_combinator = None;
      }
      '*' => {
        compound_has_other = true;
        i += 1;
        last_combinator = None;
      }
      ':' => {
        compound_has_other = true;
        i += 1;
        if i < len && chars[i] == ':' {
          i += 1;
        }
        while i < len && is_ident_char(chars[i]) {
          i += 1;
        }
        // Do NOT skip parenthesized arguments — continue scanning so that
        // type selectors inside functional pseudo-classes (e.g. `:is(div)`)
        // are also counted. Matching Stylelint v17 as-written behavior.
        last_combinator = None;
      }
      '[' => {
        compound_has_other = true;
        while i < len && chars[i] != ']' {
          i += 1;
        }
        if i < len {
          i += 1;
        }
        last_combinator = None;
      }
      '&' => {
        compound_has_other = true;
        i += 1;
        while i < len && is_ident_char(chars[i]) {
          i += 1;
        }
        last_combinator = None;
      }
      '(' | ')' => {
        // Skip parentheses themselves (from pseudo-class functions)
        // without skipping their contents
        i += 1;
      }
      _ if chars[i].is_ascii_alphabetic() || !chars[i].is_ascii() => {
        let start = i;
        while i < len && is_ident_char(chars[i]) {
          i += 1;
        }
        let name = &selector[start..i];
        let is_custom_element = name.contains('-');

        let comb = last_combinator;

        let seg_idx = segments.len();
        segments.push(SelectorSegment {
          name: name.to_string(),
          is_after_descendant_combinator: comb == Some(' '),
          is_after_child_combinator: comb == Some('>'),
          is_after_next_sibling_combinator: comb == Some('+') || comb == Some('~'),
          is_compounded: false, // will be set in finalization
          is_custom_element,
        });
        compound_type_indices.push(seg_idx);

        last_combinator = None;
      }
      _ => {
        i += 1;
      }
    }
  }

  // Finalize the last compound
  if compound_has_other {
    for &idx in &compound_type_indices {
      segments[idx].is_compounded = true;
    }
  }

  segments
}

fn is_ident_char(c: char) -> bool {
  c.is_ascii_alphanumeric() || c == '-' || c == '_' || !c.is_ascii()
}

/// Strip SCSS/Less-specific constructs from a selector string:
/// - `//` line comments
/// - `#{...}` interpolation (SCSS, replaced with empty string)
/// - `@{...}` interpolation (Less, replaced with empty string)
fn strip_preprocessor_constructs(selector: &str, syntax: gale_css_parser::Syntax) -> String {
  // Strip line comments (both whole-line and inline `// ...` comments)
  let no_comments: String = selector
    .lines()
    .map(|line| {
      // Find `//` that isn't inside a string
      let mut in_single = false;
      let mut in_double = false;
      let bytes = line.as_bytes();
      for i in 0..bytes.len() {
        if bytes[i] == b'\'' && !in_double {
          in_single = !in_single;
        } else if bytes[i] == b'"' && !in_single {
          in_double = !in_double;
        } else if !in_single
          && !in_double
          && bytes[i] == b'/'
          && i + 1 < bytes.len()
          && bytes[i + 1] == b'/'
        {
          return &line[..i];
        }
      }
      line
    })
    .collect::<Vec<_>>()
    .join("\n");

  // Then strip interpolation
  let mut result = String::with_capacity(no_comments.len());
  let chars: Vec<char> = no_comments.chars().collect();
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

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

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
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_too_many_type_selectors() {
    let d = SelectorMaxType.check(&style_with_selector("div span a ul"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(
      d[0]
        .message
        .contains("to have no more than 3 type selector")
    );
  }

  #[test]
  fn allows_within_limit() {
    let d = SelectorMaxType.check(&style_with_selector("div span a"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn does_not_count_class_selectors() {
    let d = SelectorMaxType.check(&style_with_selector(".a .b .c .d"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn does_not_count_id_selectors() {
    let d = SelectorMaxType.check(&style_with_selector("#a #b #c #d"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn max_zero_rejects_single_type() {
    let opts = serde_json::json!([0]);
    let c = ctx_with_options(&opts);
    let d = SelectorMaxType.check(&style_with_selector("foo {}"), &c);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn max_zero_accepts_class() {
    let opts = serde_json::json!([0]);
    let c = ctx_with_options(&opts);
    let d = SelectorMaxType.check(&style_with_selector(".bar {}"), &c);
    assert!(d.is_empty());
  }

  #[test]
  fn ignore_descendant() {
    let opts = serde_json::json!([0, {"ignore": ["descendant"]}]);
    let c = ctx_with_options(&opts);
    // `.foo div` — div is after descendant combinator, should be ignored
    let d = SelectorMaxType.check(&style_with_selector(".foo div"), &c);
    assert!(d.is_empty());
  }

  #[test]
  fn ignore_child() {
    let opts = serde_json::json!([0, {"ignore": ["child"]}]);
    let c = ctx_with_options(&opts);
    // `.foo > div` — div is after child combinator, should be ignored
    let d = SelectorMaxType.check(&style_with_selector(".foo > div"), &c);
    assert!(d.is_empty());
  }

  #[test]
  fn ignore_compounded() {
    let opts = serde_json::json!([0, {"ignore": ["compounded"]}]);
    let c = ctx_with_options(&opts);
    // `.foo.bar div.baz` — div.baz is compounded with class, should be ignored
    let d = SelectorMaxType.check(&style_with_selector("div.baz"), &c);
    assert!(d.is_empty());
  }

  #[test]
  fn ignore_types_exact() {
    let opts = serde_json::json!([0, {"ignoreTypes": ["fieldset"]}]);
    let c = ctx_with_options(&opts);
    let d = SelectorMaxType.check(&style_with_selector("fieldset"), &c);
    assert!(d.is_empty());
  }

  #[test]
  fn ignore_types_regex() {
    let opts = serde_json::json!([0, {"ignoreTypes": ["/^my-/"]}]);
    let c = ctx_with_options(&opts);
    let d = SelectorMaxType.check(&style_with_selector("my-type"), &c);
    assert!(d.is_empty());
  }

  #[test]
  fn counts_types_inside_is() {
    // Stylelint v17: type selectors inside :is() ARE counted.
    // `:is(div)` has 1 type selector.
    let opts = serde_json::json!([0]);
    let c = ctx_with_options(&opts);
    let d = SelectorMaxType.check(&style_with_selector(":is(div)"), &c);
    assert_eq!(d.len(), 1, "expected 1 diagnostic for type inside :is()");
  }

  #[test]
  fn counts_types_inside_not() {
    // Stylelint v17: `.foo:not(div)` counts `div` as a type selector.
    let opts = serde_json::json!([0]);
    let c = ctx_with_options(&opts);
    let d = SelectorMaxType.check(&style_with_selector(".foo:not(div)"), &c);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn counts_types_inside_has() {
    // Stylelint v17: `div:has(span a)` has 3 type selectors total.
    let d = SelectorMaxType.check(&style_with_selector("div:has(span a)"), &ctx());
    assert!(d.is_empty(), "3 types within default max of 3 should pass");

    let opts = serde_json::json!([2]);
    let c = ctx_with_options(&opts);
    let d = SelectorMaxType.check(&style_with_selector("div:has(span a)"), &c);
    assert_eq!(d.len(), 1, "3 types with max 2 should fail");
  }
}
