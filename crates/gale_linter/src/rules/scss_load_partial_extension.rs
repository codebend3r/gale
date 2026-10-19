use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::Diagnostic;

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
  /// Flags a `@use`, `@forward` or `@import` path that carries a file extension.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if !matches!(at.name.as_str(), "use" | "forward" | "import") {
      return vec![];
    }

    let path = extract_path(&at.params);
    if path.is_empty() {
      return vec![];
    }

    for ext in &[".scss", ".sass"] {
      if path.ends_with(ext) {
        return vec![
          Diagnostic::new(
            NAME,
            format!("Unexpected file extension \"{}\" in partial import", ext),
          )
          .span(at.span),
        ];
      }
    }

    vec![]
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
  use gale_css_parser::AtRule;
  use gale_diagnostics::Span;

  use crate::testing::{configured, ctx, invalid_options, scss_ctx};

  fn use_rule(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "use".to_string(),
      params: params.to_string(),
      span: Span::new(0, 10),
      children: vec![],
    })
  }

  #[test]
  fn reports_scss_extension() {
    let rule = configured(&ScssLoadPartialExtension, None);
    let d = rule.check(&use_rule("\"variables.scss\""), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains(".scss"));
  }

  #[test]
  fn reports_sass_extension() {
    let rule = configured(&ScssLoadPartialExtension, None);
    let d = rule.check(&use_rule("\"variables.sass\""), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_css_extension() {
    let rule = configured(&ScssLoadPartialExtension, None);
    let d = rule.check(&use_rule("\"reset.css\""), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_no_extension() {
    let rule = configured(&ScssLoadPartialExtension, None);
    let d = rule.check(&use_rule("\"variables\""), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssLoadPartialExtension, None);
    let ctx = ctx();
    assert!(rule.check(&use_rule("\"variables.scss\""), &ctx).is_empty());
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
