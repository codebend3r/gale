use std::collections::HashSet;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "no-duplicate-at-import-rules";

declare_rule! {
  /// Disallow duplicate `@import` rules within a stylesheet.
  ///
  /// Equivalent to Stylelint's `no-duplicate-at-import-rules` rule.
  pub struct NoDuplicateAtImportRules => NoDuplicateAtImportRulesCheck {
    name: NAME,
    description: "Disallow duplicate @import rules",
    default_severity: Warning,
  }
}

/// `no-duplicate-at-import-rules`, which takes no options.
pub struct NoDuplicateAtImportRulesCheck;

impl NoDuplicateAtImportRulesCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for NoDuplicateAtImportRulesCheck {
  /// Flags an `@import` whose target was already imported.
  fn check_root(&self, nodes: &[CssNode], _context: &RuleContext) -> Vec<Diagnostic> {
    let mut seen = HashSet::new();
    let mut diagnostics = Vec::new();

    for node in nodes {
      if let CssNode::AtRule(at_rule) = node
        && at_rule.name == "import"
      {
        let url = at_rule.params.trim().to_string();
        if !seen.insert(url.clone()) {
          diagnostics.push(
            Diagnostic::new(
              NAME,
              format!("Unexpected duplicate @import rule \"{}\"", url),
            )
            .span(Span::new(at_rule.span.offset, at_rule.span.length)),
          );
        }
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan};

  use crate::testing::{configured, ctx};

  #[test]
  fn reports_duplicate_imports() {
    let rule = configured(&NoDuplicateAtImportRules, None);
    let nodes = vec![
      CssNode::AtRule(AtRule {
        name: "import".to_string(),
        params: "reset.css".to_string(),
        span: ParserSpan::new(0, 22),
        children: vec![],
      }),
      CssNode::AtRule(AtRule {
        name: "import".to_string(),
        params: "normalize.css".to_string(),
        span: ParserSpan::new(23, 26),
        children: vec![],
      }),
      CssNode::AtRule(AtRule {
        name: "import".to_string(),
        params: "reset.css".to_string(),
        span: ParserSpan::new(50, 22),
        children: vec![],
      }),
    ];
    let diags = rule.check_root(&nodes, &ctx());
    assert_eq!(diags.len(), 1);
    assert_eq!(
      diags[0].message,
      "Unexpected duplicate @import rule \"reset.css\""
    );
  }

  #[test]
  fn ignores_unique_imports() {
    let rule = configured(&NoDuplicateAtImportRules, None);
    let nodes = vec![
      CssNode::AtRule(AtRule {
        name: "import".to_string(),
        params: "reset.css".to_string(),
        span: ParserSpan::new(0, 22),
        children: vec![],
      }),
      CssNode::AtRule(AtRule {
        name: "import".to_string(),
        params: "normalize.css".to_string(),
        span: ParserSpan::new(23, 26),
        children: vec![],
      }),
    ];
    let diags = rule.check_root(&nodes, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(invalid_options("no-duplicate-at-import-rules", serde_json::json!(true)).is_empty());
    // The rule takes nothing but `true`, and ignores secondary options.
    assert!(
      invalid_options(
        "no-duplicate-at-import-rules",
        serde_json::json!([true, { "foo": 1 }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options("no-duplicate-at-import-rules", serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"no-duplicate-at-import-rules\""]
    );
  }
}
