use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/load-partial-extension";

declare_rule! {
  /// Disallow file extensions in `@use`, `@forward`, and `@import` paths.
  pub struct ScssLoadPartialExtension => ScssLoadPartialExtensionCheck {
    name: NAME,
    description: "Disallow unnecessary file extensions in @use/@import",
    default_severity: Warning,
  }
}

/// `scss/load-partial-extension` as configured.
pub struct ScssLoadPartialExtensionCheck;

impl ScssLoadPartialExtensionCheck {
  /// Stylelint's options: `"always"` or `"never"`.  Gale implements only
  /// `"never"`, whatever the option: `"always"` is accepted but not yet
  /// applied.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.primary_one_of(&["always", "never"]);
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssLoadPartialExtensionCheck {
  /// Flags a `@use`, `@forward` or `@import` path that carries a file
  /// extension, at the extension, as stylelint-scss reports it.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      if !matches!(at.name(), "use" | "forward" | "import") {
        continue;
      }
      let params = at.params();
      let path = extract_path(params);
      if path.is_empty() {
        continue;
      }
      if let Some(ext) = [".scss", ".sass"]
        .into_iter()
        .find(|ext| path.ends_with(ext))
      {
        // stylelint-scss reports the last `.scss` in the params.
        let span = params.rfind(ext).map_or(at.span(), |at_ext| {
          Span::new(at.params_start() + at_ext, ext.len())
        });
        diags.push(
          Diagnostic::new(
            NAME,
            format!("Unexpected file extension \"{ext}\" in partial import"),
          )
          .span(span),
        );
      }
    }
    diags
  }
}

/// Extract a path string from at-rule params, stripping quotes.
fn extract_path(params: &str) -> &str {
  let trimmed = params.trim();
  let trimmed = trimmed
    .strip_prefix('"')
    .and_then(|s| s.strip_suffix('"'))
    .or_else(|| {
      trimmed
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
    })
    .unwrap_or(trimmed);
  trimmed.split_whitespace().next().unwrap_or(trimmed)
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// The reports for `@use <params>;` in `syntax`.
  fn use_rule(params: &str, syntax: Syntax) -> Vec<Diagnostic> {
    let source = format!("@use {params};");
    configured(&ScssLoadPartialExtension, None).check_source(&context(&source, syntax))
  }

  #[test]
  fn reports_scss_extension_at_the_extension() {
    let d = use_rule("\"variables.scss\"", Syntax::Scss);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains(".scss"));
    assert_eq!((d[0].span.offset, d[0].span.length), (15, 5));
  }

  #[test]
  fn reports_sass_extension() {
    assert_eq!(use_rule("\"variables.sass\"", Syntax::Scss).len(), 1);
  }

  #[test]
  fn allows_css_and_no_extension() {
    assert!(use_rule("\"reset.css\"", Syntax::Scss).is_empty());
    assert!(use_rule("\"variables\"", Syntax::Scss).is_empty());
  }

  #[test]
  fn skips_non_scss() {
    assert!(use_rule("\"variables.scss\"", Syntax::Css).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sideways")),
      ["Invalid option value \"sideways\" for rule \"scss/load-partial-extension\""]
    );
  }
}
