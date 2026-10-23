use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

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
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      if at.name() != "import" {
        continue;
      }
      // Parse the import paths from params. May be comma-separated and quoted.
      for part in at.params().split(',') {
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
        let message = match self.expectation {
          "never" if has_scss_ext => format!("Unexpected extension in @import \"{path}\""),
          "always" if !has_scss_ext => format!("Expected extension in @import \"{path}\""),
          _ => continue,
        };
        diags.push(Diagnostic::new(NAME, message).span(at.span()));
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::{Value, json};

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// The reports for `@import <params>;` in `syntax`.
  fn import(options: Option<Value>, params: &str, syntax: Syntax) -> Vec<Diagnostic> {
    let source = format!("@import {params};");
    configured(&ScssAtImportPartialExtension, options).check_source(&context(&source, syntax))
  }

  #[test]
  fn allows_import_without_extension() {
    assert!(import(None, "\"foo\"", Syntax::Scss).is_empty());
  }

  #[test]
  fn reports_import_with_scss_extension() {
    let d = import(None, "\"foo.scss\"", Syntax::Scss);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("foo.scss"));
    assert_eq!((d[0].span.offset, d[0].span.length), (0, 19));
  }

  #[test]
  fn always_reports_each_path_without_one() {
    let d = import(
      Some(json!("always")),
      "\"a\", \"b.scss\", 'c'",
      Syntax::Scss,
    );
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn allows_css_and_url_imports() {
    assert!(import(None, "\"foo.css\"", Syntax::Scss).is_empty());
    assert!(import(None, "\"https://example.com/foo.scss\"", Syntax::Scss).is_empty());
  }

  #[test]
  fn skips_non_scss() {
    assert!(import(None, "\"foo.scss\"", Syntax::Css).is_empty());
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
