use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::selector::postcss::{Kind, Node, split_selectors};
use gale_css_parser::selector::specificity::{self, Specificity, is_pseudo_element};
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{Accept, InvalidOption, Setting};
use crate::pattern;
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "selector-max-specificity";

declare_rule! {
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
  pub struct SelectorMaxSpecificity => SelectorMaxSpecificityCheck {
    name: NAME,
    description: "Limit the specificity of selectors",
    default_severity: Warning,
  }
}

/// `selector-max-specificity` as configured.
pub struct SelectorMaxSpecificityCheck {
  config: Config,
}

impl SelectorMaxSpecificityCheck {
  /// Stylelint's options: a specificity written `"a,b,c"`, then
  /// `ignoreSelectors` (names or `/regex/` entries).  Gale reads no
  /// specificity as `"0,2,0"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max = options.primary_value(&[Accept::Test(is_specificity)]);
    let ignore_selectors = options.strings("ignoreSelectors");
    options.finish()?;
    let max = max
      .and_then(serde_json::Value::as_str)
      .and_then(parse_specificity_option)
      .unwrap_or(DEFAULT_MAX);
    let ignore_selectors = ignore_selectors
      .iter()
      .map(|s| match pattern::regex_entry(s) {
        Some(re) => IgnorePattern::Regex(re),
        None => IgnorePattern::Exact(s.to_string()),
      })
      .collect();
    Ok(Self {
      config: Config {
        max,
        ignore_selectors,
      },
    })
  }
}

/// Stylelint's test of the primary option: a string of three whole numbers
/// separated by commas.
fn is_specificity(value: &serde_json::Value) -> bool {
  value.as_str().is_some_and(|text| {
    let parts: Vec<&str> = text.split(',').collect();
    parts.len() == 3
      && parts
        .iter()
        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
  })
}

const DEFAULT_MAX: Specificity = Specificity::new(0, 2, 0);

/// Parse a specificity string like `"0,2,0"`.  A count too large for a
/// `u32` stands for one no selector reaches.
fn parse_specificity_option(s: &str) -> Option<Specificity> {
  let parts: Vec<&str> = s.split(',').collect();
  if parts.len() != 3 {
    return None;
  }
  let count = |part: &str| {
    part
      .trim()
      .parse::<usize>()
      .ok()
      .map(|n| u32::try_from(n).unwrap_or(u32::MAX))
  };
  Some(Specificity::new(
    count(parts[0])?,
    count(parts[1])?,
    count(parts[2])?,
  ))
}

/// Configuration parsed from rule options.
struct Config {
  max: Specificity,
  ignore_selectors: Vec<IgnorePattern>,
}

enum IgnorePattern {
  Exact(String),
  Regex(std::sync::Arc<pattern::Regex>),
}

impl Config {
  /// Whether this pseudo-class's specificity contribution is ignored.
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
          // Try the pattern with and without the leading colon.
          let to_check = format!(":{pseudo_name}");
          if pattern::is_match(re, &to_check) || pattern::is_match(re, pseudo_name) {
            return true;
          }
        }
      }
    }
    false
  }

  /// The selector's specificity, with `ignoreSelectors` applied the way
  /// gale always has: an ignored `:is()`, `:not()`, `:has()` or `:matches()`
  /// counts nothing, an ignored `:host()` counts as a bare `:host`, and other
  /// pseudo-classes count whether ignored or not.  Stylelint instead drops
  /// any ignored node's own share, classes and types included.
  fn specificity(&self, selector: &str) -> Option<Specificity> {
    // Gale does not resolve nesting here: `&` and a suffix glued to it
    // (`&__item`) count nothing.
    let selector = without_nesting_suffixes(selector);
    let ignored = |node: &Node| {
      if node.kind != Kind::Pseudo || node.args.is_none() || is_pseudo_element(node) {
        return None;
      }
      let name = node.value.trim_start_matches(':');
      if !self.is_pseudo_ignored(name) {
        return None;
      }
      match name.to_ascii_lowercase().as_str() {
        "is" | "not" | "has" | "matches" => Some(Specificity::default()),
        "host" => Some(Specificity::new(0, 1, 0)),
        _ => None,
      }
    };
    specificity::of_text_with(&selector, Some(&ignored))
  }
}

impl Check for SelectorMaxSpecificityCheck {
  /// Flags selectors whose specificity exceeds the maximum, stripping
  /// preprocessor interpolation first.
  ///
  /// The selector is read as PostCSS stores it, without its comments, and
  /// rules that are not standard syntax are skipped, as in Stylelint.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let config = &self.config;

    let mut diags = Vec::new();
    for rule in tree.walk_rules().filter(|rule| rule.is_standard_syntax()) {
      let selector = if matches!(
        ctx.syntax,
        gale_css_parser::Syntax::Scss
          | gale_css_parser::Syntax::Sass
          | gale_css_parser::Syntax::Less
      ) {
        strip_preprocessor_interpolation(rule.selector(), ctx.syntax)
      } else {
        rule.selector().to_string()
      };
      for (_, sel) in split_selectors(&selector) {
        let sel = sel.trim();
        if sel.is_empty() {
          continue;
        }

        // Skip selectors that look like preprocessor constructs
        if sel.starts_with('%') || sel.starts_with('$') || sel.contains("#{") || sel.contains("@{")
        {
          continue;
        }

        // A selector postcss-selector-parser cannot read is skipped, as in
        // Stylelint.
        let Some(specificity) = config.specificity(sel) else {
          continue;
        };

        if specificity > config.max {
          let Specificity { a, b, c } = specificity;
          diags.push(
                    Diagnostic::new(
                        NAME,
                        format!(
                            "Expected selector \"{sel}\" to have a specificity no more than \"{},{},{}\", but got \"{a},{b},{c}\"",
                            config.max.a, config.max.b, config.max.c,
                        ),
                    )
                    .span(rule.span()),
                );
        }
      }
    }
    diags
  }
}

/// `selector` with every `&` kept but any name glued to it (`&__item`,
/// `&-suffix`) dropped.
fn without_nesting_suffixes(selector: &str) -> String {
  let mut out = String::with_capacity(selector.len());
  let mut chars = selector.chars().peekable();
  while let Some(ch) = chars.next() {
    out.push(ch);
    if ch == '&' {
      while chars.next_if(|&next| is_suffix_char(next)).is_some() {}
    }
  }
  out
}

/// What the old scanner skipped after a `&`: ASCII letters, digits, `-` and
/// `_`, and anything non-ASCII.
fn is_suffix_char(ch: char) -> bool {
  ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || !ch.is_ascii()
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

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// A rule with the selector `sel`.
  fn style_with_selector(sel: &str) -> String {
    format!("{sel} {{ color: red; }}")
  }

  #[test]
  fn reports_high_specificity() {
    let rule = configured(&SelectorMaxSpecificity, None);
    // #id has specificity 1,0,0 which exceeds default 0,2,0
    let d = rule.check_source(&context(&style_with_selector("#id"), Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("1,0,0"));
  }

  #[test]
  fn allows_low_specificity() {
    let rule = configured(&SelectorMaxSpecificity, None);
    // .a .b has specificity 0,2,0 which is within default 0,2,0
    let d = rule.check_source(&context(&style_with_selector(".a .b"), Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_too_many_classes() {
    let rule = configured(&SelectorMaxSpecificity, None);
    // .a.b.c has specificity 0,3,0 which exceeds default 0,2,0
    let d = rule.check_source(&context(&style_with_selector(".a.b.c"), Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("0,3,0"));
  }

  #[test]
  fn where_has_zero_specificity() {
    let rule = configured(&SelectorMaxSpecificity, None);
    // :where(#id) should have (0,0,0), well under the limit
    let d = rule.check_source(&context(&style_with_selector(":where(#id)"), Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn is_takes_max_argument_specificity() {
    let rule = configured(&SelectorMaxSpecificity, None);
    // :is(#id) should have (1,0,0) which exceeds limit
    let d = rule.check_source(&context(&style_with_selector(":is(#id)"), Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("1,0,0"));

    // :is(.a) should have (0,1,0) which is within default 0,2,0
    let d = rule.check_source(&context(&style_with_selector(":is(.a)"), Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn not_takes_max_argument_specificity() {
    let rule = configured(&SelectorMaxSpecificity, None);
    // :not(#id) should have (1,0,0) which exceeds limit
    let d = rule.check_source(&context(&style_with_selector(":not(#id)"), Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("1,0,0"));
  }

  #[test]
  fn custom_option_allows_higher_specificity() {
    let opts = serde_json::json!(["0,4,1"]);
    let rule = configured(&SelectorMaxSpecificity, opts.clone());
    // .a.b.c.d has specificity 0,4,0 -- within "0,4,1"
    let d = rule.check_source(&context(&style_with_selector(".a.b.c.d"), Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn custom_option_restricts_specificity() {
    let opts = serde_json::json!(["0,1,0"]);
    let rule = configured(&SelectorMaxSpecificity, opts.clone());
    // .a.b has specificity 0,2,0 -- exceeds "0,1,0"
    let d = rule.check_source(&context(&style_with_selector(".a.b"), Syntax::Css));
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
    let rule = configured(&SelectorMaxSpecificity, opts.clone());
    let d = rule.check_source(&context(&style_with_selector("span a"), Syntax::Css));
    assert!(d.is_empty(), "span a should be allowed with max 0,2,0");
  }

  #[test]
  fn parse_specificity_option_valid() {
    assert_eq!(
      parse_specificity_option("0,2,0"),
      Some(Specificity::new(0, 2, 0))
    );
    assert_eq!(
      parse_specificity_option("1, 3, 2"),
      Some(Specificity::new(1, 3, 2))
    );
  }

  #[test]
  fn parse_specificity_option_invalid() {
    assert_eq!(parse_specificity_option("abc"), None);
    assert_eq!(parse_specificity_option("0,2"), None);
    assert_eq!(parse_specificity_option("0,2,0,1"), None);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    // The primary option is tested by a function inside a list, so Stylelint
    // words its problem with "value".
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["0,3,1", { "ignoreSelectors": [":is", "/^:my-/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("0, 2, 0")),
      ["Invalid option value \"0, 2, 0\" for rule \"selector-max-specificity\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["0,2,0", { "ignoreSelectors": [1] }])
      ),
      ["Invalid value \"1\" for option \"ignoreSelectors\" of rule \"selector-max-specificity\""]
    );
  }
}
