use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/partial-no-import";

declare_rule! {
  /// Disallow `@import` in partial files (files starting with `_`).
  ///
  /// In SCSS, partial files (prefixed with `_`) should use `@use` or `@forward`
  /// instead of `@import`.
  ///
  /// Equivalent to `scss/partial-no-import`.
  pub struct ScssPartialNoImport => ScssPartialNoImportCheck {
    name: NAME,
    description: "Disallow @import in partial SCSS files",
    default_severity: Warning,
  }
}

/// `scss/partial-no-import`, which takes no options.
pub struct ScssPartialNoImportCheck;

impl ScssPartialNoImportCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssPartialNoImportCheck {
  /// Flags `@import` inside a partial file, where `@use` or `@forward` belongs.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    // Only applies to partial files (filename starts with `_`)
    let filename = ctx
      .file_path
      .rsplit('/')
      .next()
      .or_else(|| ctx.file_path.rsplit('\\').next())
      .unwrap_or(ctx.file_path);
    if !filename.starts_with('_') {
      return vec![];
    }

    tree
      .walk_at_rules()
      .filter(|at| at.name() == "import")
      .map(|at| {
        Diagnostic::new(
          NAME,
          "Unexpected @import in partial SCSS file. Use @use or @forward instead.",
        )
        .span(at.span())
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::testing::{CheckSource, configured, invalid_options};
  use gale_css_parser::Syntax;

  /// The reports for `source` in a file named `file_path`.
  fn check(file_path: &str, source: &str, syntax: Syntax) -> Vec<Diagnostic> {
    let ctx = RuleContext {
      env: Default::default(),
      file_path,
      source,
      syntax,
      cache: None,
      embedded: None,
    };
    configured(&ScssPartialNoImport, None).check_source(&ctx)
  }

  #[test]
  fn reports_import_in_partial() {
    let d = check("_partial.scss", "a {}\n@import \"foo\";", Syntax::Scss);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@import"));
    assert_eq!((d[0].span.offset, d[0].span.length), (5, 14));
  }

  #[test]
  fn allows_import_in_non_partial() {
    assert!(check("main.scss", "@import \"foo\";", Syntax::Scss).is_empty());
  }

  #[test]
  fn skips_non_import_at_rules() {
    assert!(check("_partial.scss", "@use \"foo\";", Syntax::Scss).is_empty());
  }

  #[test]
  fn skips_non_scss() {
    assert!(check("_partial.css", "@import \"foo\";", Syntax::Css).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/partial-no-import\""]
    );
  }
}
