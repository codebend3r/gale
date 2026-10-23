use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const DEFAULT_PATTERN: &str = "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$";

const NAME: &str = "selector-id-pattern";

declare_rule! {
  /// Enforces a naming pattern for ID selectors.
  ///
  /// Extracts `#id` patterns from selectors (excluding hex colors in values)
  /// and validates them against the configured regex pattern.
  ///
  /// Accepts a regex string as the primary option.
  /// Defaults to kebab-case pattern if no option is provided.
  ///
  /// Equivalent to Stylelint's `selector-id-pattern` rule.
  pub struct SelectorIdPattern => SelectorIdPatternCheck {
    name: NAME,
    description: "Specify a pattern for ID selectors",
    default_severity: Warning,
  }
}

/// `selector-id-pattern` as configured.
pub struct SelectorIdPatternCheck {
  /// The pattern every ID must match.
  pattern: Pattern,
}

impl SelectorIdPatternCheck {
  /// Stylelint's options: a regular expression, as a string or `/regex/`.
  /// Gale reads no option as kebab-case, and a pattern that does not
  /// compile is an invalid option, as Stylelint reports it.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let pattern = options.primary_pattern();
    options.finish()?;
    let pattern = match pattern {
      Some(pattern) => pattern,
      None => Pattern::compile(DEFAULT_PATTERN).expect("the default pattern compiles"),
    };
    Ok(Self { pattern })
  }
}

impl Check for SelectorIdPatternCheck {
  /// Flags ID selector names that do not match the configured regex, defaulting
  /// to kebab-case.
  ///
  /// Rules that are not standard syntax (interpolation, placeholders, Less
  /// mixins) are skipped, as Stylelint skips them.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let pattern_str = self.pattern.source.as_str();

    let mut diags = Vec::new();
    for rule in tree.walk_rules().filter(|rule| rule.is_standard_syntax()) {
      for id in extract_id_selectors(rule.selector()) {
        // Skip IDs containing SCSS interpolation #{...}
        if matches!(
          ctx.syntax,
          gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass
        ) && id.contains("#{")
        {
          continue;
        }
        if !self.pattern.is_match(&id) {
          diags.push(
            Diagnostic::new(
              NAME,
              format!("Expected ID selector \"#{id}\" to match pattern \"{pattern_str}\""),
            )
            .span(rule.span())
            .message_args([format!("#{id}"), pattern_str.to_string()]),
          );
        }
      }
    }
    diags
  }
}

/// Extract ID names from a selector string.
/// Finds `#name` patterns that are valid CSS ID selectors — skipping `#`
/// followed by only hex digits (which would be ambiguous with hex colors,
/// though in selectors this is less common).
fn extract_id_selectors(selector: &str) -> Vec<String> {
  let mut ids = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    if chars[i] == '#' {
      // Skip SCSS interpolation #{...}
      if i + 1 < len && chars[i + 1] == '{' {
        i += 2;
        let mut depth = 1;
        while i < len && depth > 0 {
          if chars[i] == '{' {
            depth += 1;
          } else if chars[i] == '}' {
            depth -= 1;
          }
          i += 1;
        }
        continue;
      }

      i += 1;
      let start = i;
      // CSS ident chars: alphanum, hyphen, underscore, non-ASCII
      while i < len
        && (chars[i].is_ascii_alphanumeric()
          || chars[i] == '-'
          || chars[i] == '_'
          || !chars[i].is_ascii())
      {
        i += 1;
      }
      if i > start {
        let name: String = chars[start..i].iter().collect();
        // Skip if name looks purely like a hex color (all hex digits, 3/4/6/8 chars)
        let is_hex = name.chars().all(|c| c.is_ascii_hexdigit());
        let hex_len = name.len();
        if !(is_hex && (hex_len == 3 || hex_len == 4 || hex_len == 6 || hex_len == 8)) {
          ids.push(name);
        }
      }
    } else {
      i += 1;
    }
  }

  ids
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  /// A rule with the selector `sel`.
  fn style_with_selector(sel: &str) -> String {
    format!("{sel} {{ color: red; }}")
  }

  #[test]
  fn reports_camel_case_id() {
    let rule = configured(&SelectorIdPattern, None);
    let d = rule.check_source(&ctx_with_source(&style_with_selector("#myId")));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myId"));
  }

  #[test]
  fn allows_kebab_case_id() {
    let rule = configured(&SelectorIdPattern, None);
    assert!(
      rule
        .check_source(&ctx_with_source(&style_with_selector("#my-id")))
        .is_empty()
    );
    assert!(
      rule
        .check_source(&ctx_with_source(&style_with_selector("#foo")))
        .is_empty()
    );
  }

  #[test]
  fn reports_underscore_id() {
    let rule = configured(&SelectorIdPattern, None);
    let d = rule.check_source(&ctx_with_source(&style_with_selector("#my_id")));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_multiple_bad_ids() {
    let rule = configured(&SelectorIdPattern, None);
    let d = rule.check_source(&ctx_with_source(&style_with_selector("#myId #anotherId")));
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn custom_pattern_camel_case() {
    let opts = serde_json::json!("^[a-z][a-zA-Z0-9]+$");
    let rule = configured(&SelectorIdPattern, opts.clone());
    // camelCase should pass
    assert!(
      rule
        .check_source(&ctx_with_source(&style_with_selector("#myId")))
        .is_empty()
    );
    // kebab-case should fail
    let d = rule.check_source(&ctx_with_source(&style_with_selector("#my-id")));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn ignores_non_style_nodes() {
    let rule = configured(&SelectorIdPattern, None);
    let d = rule.check_source(&ctx_with_source(
      "@media (min-width: 768px) { color: red; }",
    ));
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    // The rule validates no secondary options, so it ignores them.
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["/^[a-z]+$/i", { "anything": true }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(5)),
      ["Invalid option value \"5\" for rule \"selector-id-pattern\""]
    );
    let broken = invalid_options(NAME, serde_json::json!("^[a-z"));
    assert_eq!(broken.len(), 1);
    assert!(
      broken[0].starts_with("Invalid option value \"^[a-z\" for rule \"selector-id-pattern\": "),
      "{broken:?}"
    );
  }
}
