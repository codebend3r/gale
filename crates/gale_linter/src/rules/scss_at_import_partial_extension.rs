use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-import-partial-extension";

declare_rule! {
  /// Require or disallow extension in `@import` commands for partial files.
  ///
  /// Default: `"never"` — partial imports should not include the `.scss`
  /// extension.
  ///
  /// ```scss
  /// // Good (never)
  /// @import "foo";
  ///
  /// // Bad (never)
  /// @import "foo.scss";
  /// ```
  ///
  /// Equivalent to `scss/at-import-partial-extension`.
  pub struct ScssAtImportPartialExtension => ScssAtImportPartialExtensionCheck {
    name: NAME,
    description: "Require or disallow extension in @import commands",
    default_severity: Warning,
  }
}

/// `scss/at-import-partial-extension` as configured.
pub struct ScssAtImportPartialExtensionCheck {
  /// The primary option: `"always"` or `"never"`.
  expectation: &'static str,
}

impl ScssAtImportPartialExtensionCheck {
  /// Stylelint's options: `"always"` or `"never"`, `"never"` when not given.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expectation = options
      .primary_one_of(&["always", "never"])
      .unwrap_or("never");
    options.finish()?;
    Ok(Self { expectation })
  }
}

impl Check for ScssAtImportPartialExtensionCheck {
  /// Flags an `@import` whose file extension does not match the option.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if at.name != "import" {
      return vec![];
    }

    let option = self.expectation;

    let mut diags = Vec::new();

    // Parse the import paths from params. May be comma-separated and quoted.
    for part in at.params.split(',') {
      let path = part.trim().trim_matches('"').trim_matches('\'');

      // Skip URLs and CSS imports
      if path.starts_with("http://")
        || path.starts_with("https://")
        || path.starts_with("//")
        || path.ends_with(".css")
      {
        continue;
      }

      let has_scss_ext = path.ends_with(".scss") || path.ends_with(".sass");

      match option {
        "never" => {
          if has_scss_ext {
            diags.push(
              Diagnostic::new(
                NAME,
                format!("Unexpected extension in @import \"{}\"", path),
              )
              .span(Span::new(at.span.offset, at.span.length)),
            );
          }
        }
        "always" => {
          if !has_scss_ext {
            diags.push(
              Diagnostic::new(NAME, format!("Expected extension in @import \"{}\"", path))
                .span(Span::new(at.span.offset, at.span.length)),
            );
          }
        }
        _ => {}
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan};

  use crate::testing::{configured, ctx, invalid_options, scss_ctx};

  fn import(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "import".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 20),
      children: vec![],
    })
  }

  #[test]
  fn allows_import_without_extension() {
    let rule = configured(&ScssAtImportPartialExtension, None);
    assert!(rule.check(&import("\"foo\""), &scss_ctx()).is_empty());
  }

  #[test]
  fn reports_import_with_scss_extension() {
    let rule = configured(&ScssAtImportPartialExtension, None);
    let d = rule.check(&import("\"foo.scss\""), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("foo.scss"));
  }

  #[test]
  fn allows_css_import() {
    let rule = configured(&ScssAtImportPartialExtension, None);
    assert!(rule.check(&import("\"foo.css\""), &scss_ctx()).is_empty());
  }

  #[test]
  fn allows_url_import() {
    let rule = configured(&ScssAtImportPartialExtension, None);
    assert!(
      rule
        .check(&import("\"https://example.com/foo.scss\""), &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssAtImportPartialExtension, None);
    let css_ctx = ctx();
    assert!(rule.check(&import("\"foo.scss\""), &css_ctx).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sideways")),
      ["Invalid option value \"sideways\" for rule \"scss/at-import-partial-extension\""]
    );
  }
}
