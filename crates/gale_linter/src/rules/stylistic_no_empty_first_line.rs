use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/no-empty-first-line";

declare_rule! {
  /// Disallow empty first lines.
  ///
  /// Equivalent to `@stylistic/no-empty-first-line`.
  pub struct StylisticNoEmptyFirstLine => StylisticNoEmptyFirstLineCheck {
    name: NAME,
    description: "Disallow empty first lines",
    default_severity: Warning,
  }
}

/// `@stylistic/no-empty-first-line`, which takes no options.
pub struct StylisticNoEmptyFirstLineCheck;

impl StylisticNoEmptyFirstLineCheck {
  /// The plugin validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for StylisticNoEmptyFirstLineCheck {
  /// Flags a file that opens with a blank line.
  fn check(&self, _tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if ctx.source.is_empty() {
      return vec![];
    }

    // Check if the source starts with an empty first line.
    // An empty first line means the source starts with \r\n or \n.
    let starts_empty = ctx.source.starts_with('\n') || ctx.source.starts_with("\r\n");

    if starts_empty {
      // Find the length of the leading empty whitespace to remove
      let mut end = 0;
      let bytes = ctx.source.as_bytes();
      while end < bytes.len()
        && (bytes[end] == b'\n' || bytes[end] == b'\r' || bytes[end] == b' ' || bytes[end] == b'\t')
      {
        if bytes[end] == b'\n' {
          end += 1;
          break;
        }
        if bytes[end] == b'\r' && end + 1 < bytes.len() && bytes[end + 1] == b'\n' {
          end += 2;
          break;
        }
        end += 1;
      }

      vec![
        Diagnostic::new(NAME, "Unexpected empty first line")
          .span(Span::new(0, 1))
          .fix(Fix::new(
            "Remove empty first line",
            vec![Edit::new(Span::new(0, end), "")],
          )),
      ]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  #[test]
  fn reports_empty_first_line_lf() {
    let rule = configured(&StylisticNoEmptyFirstLine, None);
    let ctx = ctx_with_source("\na { color: red; }\n");
    let d = rule.check_source(&ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty first line"));
    assert!(d[0].fix.is_some());
  }

  #[test]
  fn reports_empty_first_line_crlf() {
    let rule = configured(&StylisticNoEmptyFirstLine, None);
    let ctx = ctx_with_source("\r\na { color: red; }\n");
    let d = rule.check_source(&ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty first line"));
  }

  #[test]
  fn allows_non_empty_first_line() {
    let rule = configured(&StylisticNoEmptyFirstLine, None);
    let ctx = ctx_with_source("a { color: red; }\n");
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_empty_source() {
    let rule = configured(&StylisticNoEmptyFirstLine, None);
    let ctx = ctx_with_source("");
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"@stylistic/no-empty-first-line\""]
    );
  }
}
