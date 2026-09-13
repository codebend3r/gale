use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

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
pub struct ScssAtImportPartialExtension;

impl Rule for ScssAtImportPartialExtension {
  fn name(&self) -> &'static str {
    "scss/at-import-partial-extension"
  }

  fn description(&self) -> &'static str {
    "Require or disallow extension in @import commands"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

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

    let option = ctx.primary_option_str().unwrap_or("never");

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
                self.name(),
                format!("Unexpected extension in @import \"{}\"", path),
              )
              .severity(self.default_severity())
              .span(Span::new(at.span.offset, at.span.length)),
            );
          }
        }
        "always" => {
          if !has_scss_ext {
            diags.push(
              Diagnostic::new(
                self.name(),
                format!("Expected extension in @import \"{}\"", path),
              )
              .severity(self.default_severity())
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
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

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
    assert!(
      ScssAtImportPartialExtension
        .check(&import("\"foo\""), &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_import_with_scss_extension() {
    let d = ScssAtImportPartialExtension.check(&import("\"foo.scss\""), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("foo.scss"));
  }

  #[test]
  fn allows_css_import() {
    assert!(
      ScssAtImportPartialExtension
        .check(&import("\"foo.css\""), &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_url_import() {
    assert!(
      ScssAtImportPartialExtension
        .check(&import("\"https://example.com/foo.scss\""), &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn skips_non_scss() {
    let css_ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    assert!(
      ScssAtImportPartialExtension
        .check(&import("\"foo.scss\""), &css_ctx)
        .is_empty()
    );
  }
}
