use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "function-url-no-scheme-relative";

declare_rule! {
  /// Disallow scheme-relative URLs (e.g. `//example.com/path`).
  ///
  /// Scheme-relative URLs (starting with `//`) are deprecated and should be
  /// replaced with explicit `https://` or `http://` URLs.
  ///
  /// Equivalent to Stylelint's `function-url-no-scheme-relative` rule.
  pub struct FunctionUrlNoSchemeRelative => FunctionUrlNoSchemeRelativeCheck {
    name: NAME,
    description: "Disallow scheme-relative urls",
    default_severity: Warning,
  }
}

/// `function-url-no-scheme-relative`, which takes no options.
pub struct FunctionUrlNoSchemeRelativeCheck;

impl FunctionUrlNoSchemeRelativeCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for FunctionUrlNoSchemeRelativeCheck {
  /// Checks the value of every declaration as written.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for decl in tree.walk_decls() {
      check_value(&decl.raw_value(), decl.value_start(), &mut diags);
    }
    diags
  }
}

/// Flags `url()` arguments beginning with `//`, reporting the arguments
/// between the parentheses as Stylelint does.
fn check_value(value: &str, base_offset: usize, diags: &mut Vec<Diagnostic>) {
  let lower = value.to_ascii_lowercase();
  let bytes = lower.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    if bytes[i] == b'(' && i >= 3 {
      // Check if this is a url( function.
      let paren_pos = i;
      let mut start = i;
      while start > 0
        && (bytes[start - 1].is_ascii_alphanumeric()
          || bytes[start - 1] == b'-'
          || bytes[start - 1] == b'_')
      {
        start -= 1;
      }
      let fname = &lower[start..paren_pos];
      if fname == "url" {
        // Find matching close paren.
        let after_paren = paren_pos + 1;
        let mut depth = 1i32;
        let mut close = after_paren;
        for (j, &byte) in bytes.iter().enumerate().take(len).skip(after_paren) {
          if byte == b'(' {
            depth += 1;
          } else if byte == b')' {
            depth -= 1;
            if depth == 0 {
              close = j;
              break;
            }
          }
        }
        // Extract url content, stripping quotes.
        let content = lower[after_paren..close].trim();
        // A lone quote (`url(")`) both starts and ends with it: needs two.
        let unquoted = if content.len() >= 2
          && ((content.starts_with('"') && content.ends_with('"'))
            || (content.starts_with('\'') && content.ends_with('\'')))
        {
          &content[1..content.len() - 1]
        } else {
          content
        };

        if unquoted.starts_with("//") {
          diags.push(
            Diagnostic::new(
              NAME,
              format!("Unexpected scheme-relative url \"{}\"", unquoted),
            )
            .span(Span::new(base_offset + after_paren, close - after_paren)),
          );
        }

        i = close + 1;
        continue;
      }
    }
    i += 1;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source};

  fn style_with_value(val: &str) -> String {
    format!("a {{ background: {val}; }}")
  }

  fn decl_with_value(val: &str) -> String {
    format!("@font-face {{ src: {val}; }}")
  }

  #[test]
  fn reports_scheme_relative_url() {
    let rule = configured(&FunctionUrlNoSchemeRelative, None);
    let d = rule.check_source(&ctx_with_source(&style_with_value(
      "url(//example.com/image.png)",
    )));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("//example.com"));
    // The url's arguments, as Stylelint reports them.
    assert_eq!(d[0].span, Span::new(20, 23));
  }

  #[test]
  fn reports_quoted_scheme_relative_url() {
    let rule = configured(&FunctionUrlNoSchemeRelative, None);
    let d = rule.check_source(&ctx_with_source(&style_with_value(
      "url('//example.com/image.png')",
    )));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn accepts_absolute_url() {
    let rule = configured(&FunctionUrlNoSchemeRelative, None);
    let d = rule.check_source(&ctx_with_source(&style_with_value(
      "url(https://example.com/image.png)",
    )));
    assert!(d.is_empty());
  }

  #[test]
  fn accepts_relative_url() {
    let rule = configured(&FunctionUrlNoSchemeRelative, None);
    let d = rule.check_source(&ctx_with_source(&style_with_value("url(images/bg.png)")));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_scheme_relative_in_declaration_node() {
    let rule = configured(&FunctionUrlNoSchemeRelative, None);
    let d = rule.check_source(&ctx_with_source(&decl_with_value(
      "url(//cdn.example.com/font.woff)",
    )));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn ignores_non_url_function() {
    let rule = configured(&FunctionUrlNoSchemeRelative, None);
    let d = rule.check_source(&ctx_with_source(&style_with_value("rgb(0, 0, 0)")));
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // The rule takes nothing but `true`, and ignores secondary options.
    assert!(invalid_options(NAME, serde_json::json!([true, { "foo": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"function-url-no-scheme-relative\""]
    );
  }
}
