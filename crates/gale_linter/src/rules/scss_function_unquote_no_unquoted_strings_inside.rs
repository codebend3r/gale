use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

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
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    tree
      .walk_decls()
      .filter(|decl| has_unquoted_argument(decl.written_value()))
      .map(|decl| {
        Diagnostic::new(NAME, "Unexpected unquoted string inside unquote()").span(decl.span())
      })
      .collect()
  }
}

/// Whether the first `unquote(` in `value` (as written) takes something
/// other than a string, a variable or nothing.
fn has_unquoted_argument(value: &str) -> bool {
  // Simple pattern: look for `unquote(` followed by something that is NOT a quote
  let Some(idx) = value.find("unquote(") else {
    return false;
  };
  let trimmed = value[idx + 8..].trim_start();
  !trimmed.is_empty()
    && !trimmed.starts_with('"')
    && !trimmed.starts_with('\'')
    && !trimmed.starts_with('$')
    && !trimmed.starts_with(')')
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, invalid_options, scss_ctx_with_source};

  /// What the rule reports for `source`, in SCSS.
  fn check(source: &str) -> Vec<Diagnostic> {
    let rule = configured(&ScssFunctionUnquoteNoUnquotedStringsInside, None);
    rule.check_source(&scss_ctx_with_source(source))
  }

  #[test]
  fn reports_unquoted_string_inside_unquote() {
    let d = check("$font: unquote(bold);");
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].span.offset, d[0].span.length), (0, 21));
  }

  #[test]
  fn allows_quoted_inside_unquote() {
    assert!(check("$font: unquote(\"bold\");").is_empty());
  }

  #[test]
  fn allows_variable_inside_unquote() {
    assert!(check("$font: unquote($var);").is_empty());
  }

  #[test]
  fn looks_at_every_declaration() {
    assert_eq!(
      check("@mixin m { font-family: unquote(bold); }\na { @media x { b: unquote(c); } }").len(),
      2
    );
    assert_eq!(check("a { font-family: unquote(bold); }").len(), 1);
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
