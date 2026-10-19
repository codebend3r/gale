use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

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
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
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

    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if at.name != "import" {
      return vec![];
    }

    vec![
      Diagnostic::new(
        NAME,
        "Unexpected @import in partial SCSS file. Use @use or @forward instead.".to_string(),
      )
      .span(Span::new(at.span.offset, at.span.length)),
    ]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::testing::{configured, invalid_options};
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn partial_ctx() -> RuleContext<'static> {
    RuleContext {
      env: Default::default(),
      file_path: "_partial.scss",
      source: "",
      syntax: Syntax::Scss,
      cache: None,
    }
  }

  fn non_partial_ctx() -> RuleContext<'static> {
    RuleContext {
      env: Default::default(),
      file_path: "main.scss",
      source: "",
      syntax: Syntax::Scss,
      cache: None,
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
  fn reports_import_in_partial() {
    let rule = configured(&ScssPartialNoImport, None);
    let d = rule.check(&import("\"foo\""), &partial_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@import"));
  }

  #[test]
  fn allows_import_in_non_partial() {
    let rule = configured(&ScssPartialNoImport, None);
    assert!(
      rule
        .check(&import("\"foo\""), &non_partial_ctx())
        .is_empty()
    );
  }

  #[test]
  fn skips_non_import_at_rules() {
    let rule = configured(&ScssPartialNoImport, None);
    let node = CssNode::AtRule(AtRule {
      name: "use".to_string(),
      params: "\"foo\"".to_string(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    });
    assert!(rule.check(&node, &partial_ctx()).is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssPartialNoImport, None);
    let css_ctx = RuleContext {
      env: Default::default(),
      file_path: "_partial.css",
      source: "",
      syntax: Syntax::Css,
      cache: None,
    };
    assert!(rule.check(&import("\"foo\""), &css_ctx).is_empty());
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
