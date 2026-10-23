use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::{DeclRef, PostcssTree};
use gale_diagnostics::{Diagnostic, Span};
use std::collections::HashSet;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const DEFAULT_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z0-9]+)*$";

const NAME: &str = "scss/dollar-variable-pattern";

declare_rule! {
  /// Specify a pattern for SCSS `$variable` names.
  ///
  /// Accepts a regex string as the primary option. Defaults to kebab-case.
  /// The `$` prefix is stripped before matching.
  ///
  /// Secondary option `ignore`: `"local"` skips variables declared inside a
  /// block, `"global"` those at the top level.
  ///
  /// Secondary option `ignoreInside` (a gale extension, which upstream does
  /// not take):
  /// - `"at-rule"` or `"inside-at-rule"` — ignore variables declared inside any
  ///   at-rule (`@each`, `@for`, `@while`, `@if`, `@mixin`, `@function`, etc.)
  ///
  /// By default, variables that are loop iterators in `@each` and `@for`
  /// declarations are always ignored (they are not real variable declarations).
  ///
  /// Equivalent to `scss/dollar-variable-pattern`.
  pub struct ScssDollarVariablePattern => ScssDollarVariablePatternCheck {
    name: NAME,
    description: "Specify a pattern for SCSS dollar variable names",
    default_severity: Warning,
  }
}

/// `scss/dollar-variable-pattern` as configured.
pub struct ScssDollarVariablePatternCheck {
  /// The pattern every variable name (without the `$`) must match.
  pattern: Pattern,
  /// `ignoreInside: "at-rule"`: variables inside any at-rule are skipped.
  ignore_inside_at_rule: bool,
  /// `ignore: ["local"]`: variables inside a block are skipped.
  ignore_local: bool,
  /// `ignore: ["global"]`: top-level variables are skipped.
  ignore_global: bool,
  /// The `message` every rule accepts, which this rule words its problems
  /// with itself.
  custom_message: Option<String>,
}

impl ScssDollarVariablePatternCheck {
  /// Stylelint's options: a regular expression, then `ignore: ["local",
  /// "global"]`.  Gale also keeps the `ignoreInside` it has always taken.
  /// A pattern that does not compile is an invalid option, as Stylelint
  /// reports it.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let pattern = options.primary_pattern();
    let ignore = options.keywords("ignore", &["local", "global"]);
    let ignore_inside = options.keywords("ignoreInside", &["at-rule", "inside-at-rule"]);
    options.finish()?;
    let pattern = match pattern {
      Some(pattern) => pattern,
      None => Pattern::compile(DEFAULT_PATTERN).expect("the default pattern compiles"),
    };
    let custom_message = setting
      .secondary()
      .and_then(|secondary| secondary.get("message"))
      .and_then(|message| message.as_str())
      .map(str::to_string);
    Ok(Self {
      pattern,
      ignore_inside_at_rule: !ignore_inside.is_empty(),
      ignore_local: ignore.contains(&"local"),
      ignore_global: ignore.contains(&"global"),
      custom_message,
    })
  }
}

impl Check for ScssDollarVariablePatternCheck {
  /// Flags `$variable` names that do not match the configured regex. Loop
  /// iterators in `@each`/`@for` are never checked, being bindings, not declarations.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    // Every declaration, as stylelint-scss's `root.walkDecls` visits them.
    tree
      .walk_decls()
      .filter(|&decl| self.is_problem(decl))
      .map(|decl| {
        let message = self
          .custom_message
          .clone()
          .unwrap_or_else(|| "Expected $ variable name to match specified pattern".to_string());
        // stylelint-scss reports the variable (`word: prop`).
        Diagnostic::new(NAME, message).span(Span::new(decl.prop_start(), decl.prop().len()))
      })
      .collect()
  }
}

impl ScssDollarVariablePatternCheck {
  /// Whether `decl` declares a `$variable` whose name the rule rejects.
  fn is_problem(&self, decl: DeclRef) -> bool {
    let Some(var_name) = decl.prop().strip_prefix('$') else {
      return false;
    };
    if var_name.is_empty() {
      return false;
    }

    // Skip the loop iterator variables themselves (e.g., $color in
    // `@each $color in $list`). Other variables declared inside loops
    // are still checked, matching Stylelint-scss behavior.
    let mut loop_vars = HashSet::new();
    let mut inside_at_rule = false;
    for at_rule in decl.ancestors().filter_map(|node| node.as_at_rule()) {
      inside_at_rule = true;
      let name = at_rule.name();
      if name.eq_ignore_ascii_case("each") {
        // @each $var1, $var2 in $list
        extract_each_vars(at_rule.params(), &mut loop_vars);
      } else if name.eq_ignore_ascii_case("for") {
        // @for $i from 1 through 10
        extract_for_var(at_rule.params(), &mut loop_vars);
      }
    }
    if loop_vars.contains(var_name) {
      return false;
    }

    // Skip all variables inside at-rules when ignoreInside is set
    if self.ignore_inside_at_rule && inside_at_rule {
      return false;
    }

    // Skip local (non-top-level) variables when ignore: "local", and
    // top-level ones when ignore: "global"
    let inside_block = decl.parent().is_some();
    if (self.ignore_local && inside_block) || (self.ignore_global && !inside_block) {
      return false;
    }

    !self.pattern.is_match(var_name)
  }
}

/// Extract variable names from `@each` params.
/// e.g. `"$breakpoint-name, $breakpoint-value in $grid-breakpoints"`
/// => `{"breakpoint-name", "breakpoint-value"}`
fn extract_each_vars(params: &str, vars: &mut HashSet<String>) {
  // Everything before " in " is the binding list
  let binding_part = if let Some(idx) = params.find(" in ") {
    &params[..idx]
  } else {
    params
  };

  for part in binding_part.split(',') {
    let trimmed = part.trim();
    if let Some(name) = trimmed.strip_prefix('$')
      && !name.is_empty()
    {
      vars.insert(name.to_string());
    }
  }
}

/// Extract the variable name from `@for` params.
/// e.g. `"$i from 1 through 10"` => `{"i"}`
fn extract_for_var(params: &str, vars: &mut HashSet<String>) {
  let trimmed = params.trim();
  if let Some(rest) = trimmed.strip_prefix('$') {
    // Take until whitespace (the variable name)
    let name = rest.split_whitespace().next().unwrap_or("");
    if !name.is_empty() {
      vars.insert(name.to_string());
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{
    CheckSource, configured, ctx_with_source, invalid_options, scss_ctx_with_source,
  };

  /// What the rule, configured with `options`, reports for `source` in
  /// SCSS.
  fn check(options: Option<serde_json::Value>, source: &str) -> Vec<Diagnostic> {
    configured(&ScssDollarVariablePattern, options).check_source(&scss_ctx_with_source(source))
  }

  #[test]
  fn allows_kebab_case() {
    assert!(check(None, "$my-var: red;").is_empty());
  }

  #[test]
  fn reports_non_matching_pattern() {
    let source = "a { b: c; }\n$myVar: red;";
    let d = check(None, source);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected $ variable name"));
    assert_eq!(&source[d[0].span.offset..d[0].span.end()], "$myVar");
  }

  #[test]
  fn skips_non_dollar_declarations() {
    assert!(check(None, "a { color: red; }").is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssDollarVariablePattern, None);
    assert!(
      rule
        .check_source(&ctx_with_source("$myVar: red;"))
        .is_empty()
    );
  }

  // -- Loop variable tests --

  #[test]
  fn ignores_each_loop_variable() {
    let source = "@each $breakpoint-name in $grid-breakpoints { $breakpoint-name: \"test\"; }";
    assert!(check(None, source).is_empty());
  }

  #[test]
  fn ignores_each_loop_multiple_variables() {
    let source = "@each $name, $value in $map { $name: test; $value: test; }";
    assert!(check(None, source).is_empty());
  }

  #[test]
  fn ignores_for_loop_variable() {
    assert!(check(None, "@for $i from 1 through 10 { $i: \"override\"; }").is_empty());
  }

  #[test]
  fn reports_non_loop_vars_inside_each_loop() {
    // $otherBad is NOT the loop iterator — should still be checked.
    let d = check(None, "@each $name in $list { $otherBad: \"test\"; }");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected $ variable name"));
  }

  // -- ignoreInside tests --

  #[test]
  fn ignore_inside_at_rule_skips_all_vars_in_at_rules() {
    let opts = serde_json::json!(["^pf-v[56]-", {"ignoreInside": "at-rule"}]);
    // Variable inside a @mixin — should be ignored
    assert!(check(Some(opts), "@mixin my-mixin { $local-var: red; }").is_empty());
  }

  #[test]
  fn ignore_inside_at_rule_still_reports_top_level() {
    let opts = serde_json::json!(["^pf-v[56]-", {"ignoreInside": "at-rule"}]);
    // Top-level variable that doesn't match — should still be reported
    let d = check(Some(opts), "$bad-name: red;");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected $ variable name"));
  }

  #[test]
  fn ignore_inside_inside_at_rule_synonym() {
    let opts = serde_json::json!(["^pf-", {"ignoreInside": "inside-at-rule"}]);
    assert!(
      check(
        Some(opts),
        "@function my-fn() { $local: 1; @return $local; }"
      )
      .is_empty()
    );
  }

  // -- Helper extraction tests --

  #[test]
  fn extract_each_vars_single() {
    let mut vars = HashSet::new();
    extract_each_vars("$name in $list", &mut vars);
    assert!(vars.contains("name"));
    assert_eq!(vars.len(), 1);
  }

  #[test]
  fn extract_each_vars_multiple() {
    let mut vars = HashSet::new();
    extract_each_vars("$key, $value in $map", &mut vars);
    assert!(vars.contains("key"));
    assert!(vars.contains("value"));
    assert_eq!(vars.len(), 2);
  }

  #[test]
  fn extract_for_var_basic() {
    let mut vars = HashSet::new();
    extract_for_var("$i from 1 through 10", &mut vars);
    assert!(vars.contains("i"));
    assert_eq!(vars.len(), 1);
  }

  // -- PatternFly FP regression tests --

  #[test]
  fn ignores_local_var_and_reassignment_inside_each() {
    let source = "@each $breakpoint, $breakpoint-value in $pf-v6-map {\n  $breakpoint-name: \"\";\n  $breakpoint: \"md\";\n}";
    assert!(check(None, source).is_empty());
  }

  #[test]
  fn ignores_vars_in_nested_at_rule_inside_each() {
    let source = "@each $name in $list {\n  @if $name != \"base\" {\n    $breakpoint-name: -on-#{$name};\n  }\n}";
    assert!(check(None, source).is_empty());
  }

  #[test]
  fn ignores_vars_inside_for_loop() {
    assert!(check(None, "@for $i from 1 through 10 {\n  $width: $i * 10%;\n}").is_empty());
  }

  #[test]
  fn still_reports_non_matching_vars_inside_and_outside_loop() {
    // Both $myBadVar and $localVar should be reported (neither is a loop iterator)
    let source = "$myBadVar: red;\n@each $name in $list { $localVar: \"test\"; }";
    assert_eq!(check(None, source).len(), 2);
  }

  #[test]
  fn reports_non_matching_var_inside_mixin_without_ignore_option() {
    // Without ignoreInside, vars in @mixin (non-loop) should still be checked
    let d = check(None, "@mixin my-mixin { $myBadVar: red; }");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected $ variable name"));
  }

  #[test]
  fn checks_variables_in_at_rules_nested_in_style_rules() {
    // stylelint-scss walks every declaration.
    let source = "a { @if $x { $badVar: 1; } @include m { $otherBad: 2; } }";
    assert_eq!(check(None, source).len(), 2);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("^[a-z]+$")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("/^[a-z]+$/i")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(5)),
      ["Invalid option value \"5\" for rule \"scss/dollar-variable-pattern\""]
    );
    let broken = invalid_options(NAME, serde_json::json!("^[a-z"));
    assert_eq!(broken.len(), 1);
    assert!(
      broken[0]
        .starts_with("Invalid option value \"^[a-z\" for rule \"scss/dollar-variable-pattern\": "),
      "{broken:?}"
    );
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["^[a-z]", { "ignore": ["local", "global"] }])
      )
      .is_empty()
    );
    // `ignoreInside` is a gale extension, kept accepted.
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["^[a-z]", { "ignoreInside": "at-rule" }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(["^[a-z]", { "ignore": "inside" }])),
      ["Invalid value \"inside\" for option \"ignore\" of rule \"scss/dollar-variable-pattern\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(["^[a-z]", { "except": ["local"] }])),
      ["Invalid option name \"except\" for rule \"scss/dollar-variable-pattern\""]
    );
  }

  #[test]
  fn ignore_local_and_global() {
    let source = "$Top: 1;\na {\n  $Inner: 2;\n}\n";
    let lint = |options: serde_json::Value| {
      crate::testing::lint(NAME, options, source, gale_css_parser::Syntax::Scss).len()
    };
    // A bare object is how the config gives `true` with secondary options:
    // the default pattern.
    assert_eq!(lint(serde_json::json!({})), 2);
    assert_eq!(
      lint(serde_json::json!([DEFAULT_PATTERN, { "ignore": ["local"] }])),
      1
    );
    assert_eq!(
      lint(serde_json::json!([DEFAULT_PATTERN, { "ignore": "global" }])),
      1
    );
    assert_eq!(
      lint(serde_json::json!([DEFAULT_PATTERN, { "ignore": ["local", "global"] }])),
      0
    );
  }
}
