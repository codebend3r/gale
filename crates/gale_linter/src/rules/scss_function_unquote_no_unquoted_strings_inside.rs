use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/function-unquote-no-unquoted-strings-inside";

declare_rule! {
  /// Disallow unquoted strings inside `unquote()`.
  pub struct ScssFunctionUnquoteNoUnquotedStringsInside => ScssFunctionUnquoteNoUnquotedStringsInsideCheck {
    name: NAME,
    description: "Disallow unquoted strings inside unquote()",
    default_severity: Warning,
  }
}

/// `scss/function-unquote-no-unquoted-strings-inside`, which takes no options.
pub struct ScssFunctionUnquoteNoUnquotedStringsInsideCheck;

impl ScssFunctionUnquoteNoUnquotedStringsInsideCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssFunctionUnquoteNoUnquotedStringsInsideCheck {
  /// Flags `unquote()` calls whose argument is already unquoted.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::Declaration(decl) = node else {
      return vec![];
    };

    let value = &decl.value;
    // Simple pattern: look for `unquote(` followed by something that is NOT a quote
    if let Some(idx) = value.find("unquote(") {
      let after = &value[idx + 8..];
      let trimmed = after.trim_start();
      if !trimmed.is_empty()
        && !trimmed.starts_with('"')
        && !trimmed.starts_with('\'')
        && !trimmed.starts_with('$')
        && !trimmed.starts_with(')')
      {
        return vec![
          Diagnostic::new(NAME, "Unexpected unquoted string inside unquote()")
            .span(Span::new(decl.span.offset, decl.span.length)),
        ];
      }
    }

    vec![]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan};

  use crate::testing::{configured, invalid_options, scss_ctx};

  fn decl(value: &str) -> CssNode {
    CssNode::Declaration(Declaration {
      property: "font-family".to_string(),
      value: value.to_string(),
      span: ParserSpan::new(0, 10),
      important: false,
    })
  }

  #[test]
  fn reports_unquoted_string_inside_unquote() {
    let rule = configured(&ScssFunctionUnquoteNoUnquotedStringsInside, None);
    let d = rule.check(&decl("unquote(bold)"), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_quoted_inside_unquote() {
    let rule = configured(&ScssFunctionUnquoteNoUnquotedStringsInside, None);
    let d = rule.check(&decl("unquote(\"bold\")"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_variable_inside_unquote() {
    let rule = configured(&ScssFunctionUnquoteNoUnquotedStringsInside, None);
    let d = rule.check(&decl("unquote($var)"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      [
        "Unexpected option value \"always\" for rule \"scss/function-unquote-no-unquoted-strings-inside\""
      ]
    );
  }
}
