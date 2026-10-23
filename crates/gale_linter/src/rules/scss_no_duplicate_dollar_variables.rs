use std::collections::HashSet;

use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::{DeclRef, PostcssTree};
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
  ///
  /// The scope is the whole file: every variable at the top level or in
  /// at-rules (however deep) is compared with every other, and those in
  /// style rules are never looked at.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let mut diagnostics = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for decl in tree.walk_decls() {
      let name = decl.prop();
      if !name.starts_with('$')
        || decl.ancestors().any(|node| node.as_at_rule().is_none())
        || self.is_ignored(decl)
      {
        continue;
      }
      if !seen.insert(name) {
        // stylelint-scss reports the variable (`word: decl.prop`).
        diagnostics.push(
          Diagnostic::new(
            NAME,
            format!("Unexpected duplicate dollar variable \"{}\"", name),
          )
          .span(Span::new(decl.prop_start(), name.len())),
        );
      }
    }
    diagnostics
  }
}

impl ScssNoDuplicateDollarVariablesCheck {
  /// Whether `ignoreInside` or `ignoreInsideAtRules` skips `decl`, by the
  /// at-rule it sits directly in.  postcss-scss makes `@else` a sibling of
  /// its `@if`, named `else`, so an `@else` block is nested only when its
  /// `@if` is.
  fn is_ignored(&self, decl: DeclRef) -> bool {
    let Some(at_rule) = decl.parent().and_then(|parent| parent.as_at_rule()) else {
      return false;
    };
    let nested = at_rule.parent().is_some();
    self.ignore_inside == Some("at-rule")
      || (nested && self.ignore_inside == Some("nested-at-rule"))
      || self.ignore_inside_at_rules.matches(at_rule.name())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{
    CheckSource, configured, ctx_with_source, invalid_options, scss_ctx_with_source,
  };

  /// What the rule reports for `source` in SCSS.
  fn check(source: &str) -> Vec<Diagnostic> {
    configured(&ScssNoDuplicateDollarVariables, None).check_source(&scss_ctx_with_source(source))
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssNoDuplicateDollarVariables, None);
    let source = "$color: red;\n$color: red;";
    assert!(rule.check_source(&ctx_with_source(source)).is_empty());
  }

  #[test]
  fn reports_duplicate_dollar_variable() {
    let source = "$color: red;\n$size: red;\n$color: red;";
    let d = check(source);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("$color"));
    assert_eq!((d[0].span.offset, d[0].span.length), (25, 6));
  }

  #[test]
  fn allows_unique_dollar_variables() {
    assert!(check("$color: red;\n$size: red;\n$weight: red;").is_empty());
  }

  #[test]
  fn ignores_non_dollar_declarations() {
    assert!(check("@mixin m { color: red; color: blue; }").is_empty());
  }

  #[test]
  fn leaves_style_rules_alone() {
    assert!(check("$a: 1;\na { $a: 2; @if x { $a: 3; } }").is_empty());
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

  #[test]
  fn an_else_block_sits_beside_its_if() {
    // postcss-scss makes `@else` a top-level at-rule named `else`, so it
    // is not nested and `ignoreInsideAtRules: ["else"]` covers `@else if`.
    let source = "$a: 1;\n@if x { $b: 1; } @else if y { $a: 2; } @else { $a: 3; }\n";
    let lint = |options: serde_json::Value| {
      crate::testing::lint(NAME, options, source, gale_css_parser::Syntax::Scss).len()
    };
    assert_eq!(lint(serde_json::json!(true)), 2);
    assert_eq!(
      lint(serde_json::json!([true, { "ignoreInside": "nested-at-rule" }])),
      2
    );
    assert_eq!(
      lint(serde_json::json!([true, { "ignoreInsideAtRules": ["else"] }])),
      0
    );
  }
}
