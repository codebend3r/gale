use std::collections::HashMap;

use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};
use serde_json::Value;

use crate::lint_env::LintEnv;
use crate::options::{Accept, InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/no-duplicate-dollar-variables";

declare_rule! {
  /// Disallow duplicate dollar variables within the same scope.
  ///
  /// Equivalent to `scss/no-duplicate-dollar-variables`.
  pub struct ScssNoDuplicateDollarVariables => ScssNoDuplicateDollarVariablesCheck {
    name: NAME,
    description: "Disallow duplicate dollar variables within a scope",
    default_severity: Warning,
  }
}

/// `scss/no-duplicate-dollar-variables` as configured.
pub struct ScssNoDuplicateDollarVariablesCheck {
  /// `ignoreInside`: `"at-rule"` skips variables directly inside any
  /// at-rule, `"nested-at-rule"` those inside an at-rule that is itself
  /// nested.
  ignore_inside: Option<&'static str>,
  /// `ignoreInsideAtRules`: variables directly inside these at-rules are
  /// skipped.
  ignore_inside_at_rules: IgnoreInsideAtRules,
}

/// `ignoreInsideAtRules` as upstream applies it: `includes` on what the
/// config gives, so a string matches any at-rule name it contains and a
/// list matches the names it lists.
enum IgnoreInsideAtRules {
  /// Not given.
  None,
  /// A single string, which matches by substring.
  Text(String),
  /// A list of names, each matched exactly.
  Names(Vec<String>),
}

impl IgnoreInsideAtRules {
  /// Whether variables directly inside `@name` are skipped.
  fn matches(&self, name: &str) -> bool {
    match self {
      Self::None => false,
      Self::Text(text) => text.contains(name),
      Self::Names(names) => names.iter().any(|listed| listed == name),
    }
  }
}

impl ScssNoDuplicateDollarVariablesCheck {
  /// Stylelint's options: `true`, then `ignoreInside` (`"at-rule"` or
  /// `"nested-at-rule"`), `ignoreInsideAtRules` (names) and
  /// `ignoreDefaults` (a boolean, accepted but not yet applied: gale does
  /// not treat `!default` assignments specially).
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    // Upstream compares the option itself with each keyword, so a list
    // passes validation but turns nothing off.
    let ignore_inside = options
      .value(
        "ignoreInside",
        &[
          Accept::Exactly("at-rule"),
          Accept::Exactly("nested-at-rule"),
        ],
      )
      .and_then(|value| match value.as_str() {
        Some("at-rule") => Some("at-rule"),
        Some("nested-at-rule") => Some("nested-at-rule"),
        _ => None,
      });
    let ignore_inside_at_rules = match options.value("ignoreInsideAtRules", &[Accept::String]) {
      Some(Value::String(text)) => IgnoreInsideAtRules::Text(text.clone()),
      Some(Value::Array(names)) => IgnoreInsideAtRules::Names(
        names
          .iter()
          .filter_map(|name| name.as_str().map(str::to_string))
          .collect(),
      ),
      _ => IgnoreInsideAtRules::None,
    };
    options.flag("ignoreDefaults");
    options.finish()?;
    Ok(Self {
      ignore_inside,
      ignore_inside_at_rules,
    })
  }
}

impl Check for ScssNoDuplicateDollarVariablesCheck {
  /// Flags a `$variable` assigned more than once in the same scope.
  fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let mut diagnostics = Vec::new();
    let mut seen: HashMap<String, Span> = HashMap::new();
    self.collect_dollar_vars(nodes, None, &mut seen, &mut diagnostics);
    diagnostics
  }
}

impl ScssNoDuplicateDollarVariablesCheck {
  /// Recurses through the tree, reporting each repeat assignment of a name
  /// already recorded in `seen`.  `parent` is the at-rule holding `nodes`
  /// and whether that at-rule is itself nested, or `None` at the root.
  fn collect_dollar_vars(
    &self,
    nodes: &[CssNode],
    parent: Option<(&str, bool)>,
    seen: &mut HashMap<String, Span>,
    diagnostics: &mut Vec<Diagnostic>,
  ) {
    for node in nodes {
      match node {
        CssNode::Declaration(decl) if decl.property.starts_with('$') => {
          if let Some((at_rule, nested)) = parent
            && (self.ignore_inside == Some("at-rule")
              || (nested && self.ignore_inside == Some("nested-at-rule"))
              || self.ignore_inside_at_rules.matches(at_rule))
          {
            continue;
          }
          let var_name = decl.property.clone();
          if seen.contains_key(&var_name) {
            diagnostics.push(
              Diagnostic::new(
                NAME,
                format!("Unexpected duplicate dollar variable \"{}\"", var_name),
              )
              .span(decl.span),
            );
          } else {
            seen.insert(var_name, decl.span);
          }
        }
        CssNode::AtRule(at) => {
          // Each at-rule block gets its own scope for dollar variables,
          // but we still track the outer scope's variables. For simplicity
          // (matching the default stylelint-scss behaviour), we check at
          // the top-level scope only and recurse for nested at-rules.
          self.collect_dollar_vars(
            &at.children,
            Some((&at.name, parent.is_some())),
            seen,
            diagnostics,
          );
        }
        _ => {}
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Declaration;

  use crate::testing::{configured, ctx, invalid_options, scss_ctx};

  fn dollar_var(name: &str) -> CssNode {
    CssNode::Declaration(Declaration {
      property: name.to_string(),
      value: "red".to_string(),
      span: Span::new(0, 10),
      important: false,
    })
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssNoDuplicateDollarVariables, None);
    let nodes = vec![dollar_var("$color"), dollar_var("$color")];
    assert!(rule.check_root(&nodes, &ctx()).is_empty());
  }

  #[test]
  fn reports_duplicate_dollar_variable() {
    let rule = configured(&ScssNoDuplicateDollarVariables, None);
    let nodes = vec![
      dollar_var("$color"),
      dollar_var("$size"),
      dollar_var("$color"),
    ];
    let d = rule.check_root(&nodes, &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("$color"));
  }

  #[test]
  fn allows_unique_dollar_variables() {
    let rule = configured(&ScssNoDuplicateDollarVariables, None);
    let nodes = vec![
      dollar_var("$color"),
      dollar_var("$size"),
      dollar_var("$weight"),
    ];
    let d = rule.check_root(&nodes, &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_non_dollar_declarations() {
    let rule = configured(&ScssNoDuplicateDollarVariables, None);
    let nodes = vec![
      CssNode::Declaration(Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: Span::new(0, 10),
        important: false,
      }),
      CssNode::Declaration(Declaration {
        property: "color".to_string(),
        value: "blue".to_string(),
        span: Span::new(20, 10),
        important: false,
      }),
    ];
    let d = rule.check_root(&nodes, &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!([true, { "ignoreInside": "at-rule", "ignoreInsideAtRules": ["if"], "ignoreDefaults": false }])).is_empty());
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreInside": ["nested-at-rule"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/no-duplicate-dollar-variables\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignoreInside": "block" }])),
      [
        "Invalid value \"block\" for option \"ignoreInside\" of rule \"scss/no-duplicate-dollar-variables\""
      ]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignoreDefaults": 1 }])),
      [
        "Invalid value \"1\" for option \"ignoreDefaults\" of rule \"scss/no-duplicate-dollar-variables\""
      ]
    );
  }

  #[test]
  fn ignore_inside_options_skip_variables_in_at_rules() {
    let source = "$a: 1;\n@if true {\n  $a: 2;\n  @if true {\n    $a: 3;\n  }\n}\n";
    let lint = |options: serde_json::Value| {
      crate::testing::lint(NAME, options, source, gale_css_parser::Syntax::Scss).len()
    };
    assert_eq!(lint(serde_json::json!(true)), 2);
    assert_eq!(
      lint(serde_json::json!([true, { "ignoreInside": "at-rule" }])),
      0
    );
    assert_eq!(
      lint(serde_json::json!([true, { "ignoreInside": "nested-at-rule" }])),
      1
    );
    assert_eq!(
      lint(serde_json::json!([true, { "ignoreInsideAtRules": ["if"] }])),
      0
    );
    assert_eq!(
      lint(serde_json::json!([true, { "ignoreInsideAtRules": ["each"] }])),
      2
    );
  }
}
