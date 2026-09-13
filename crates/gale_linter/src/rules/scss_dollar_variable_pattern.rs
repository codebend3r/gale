use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;
use std::collections::HashSet;

use crate::rule::{Rule, RuleContext};

const DEFAULT_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z0-9]+)*$";

/// Specify a pattern for SCSS `$variable` names.
///
/// Accepts a regex string as the primary option. Defaults to kebab-case.
/// The `$` prefix is stripped before matching.
///
/// Secondary option `ignoreInside`:
/// - `"at-rule"` or `"inside-at-rule"` — ignore variables declared inside any
///   at-rule (`@each`, `@for`, `@while`, `@if`, `@mixin`, `@function`, etc.)
///
/// By default, variables that are loop iterators in `@each` and `@for`
/// declarations are always ignored (they are not real variable declarations).
///
/// Equivalent to `scss/dollar-variable-pattern`.
pub struct ScssDollarVariablePattern;

impl Rule for ScssDollarVariablePattern {
  fn name(&self) -> &'static str {
    "scss/dollar-variable-pattern"
  }

  fn description(&self) -> &'static str {
    "Specify a pattern for SCSS dollar variable names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let pattern_str = ctx.primary_option_str().unwrap_or(DEFAULT_PATTERN);
    let re = match Regex::new(pattern_str) {
      Ok(r) => r,
      Err(_) => return vec![],
    };

    let secondary = ctx.secondary_options();
    let ignore_inside_at_rule = match &secondary {
      Some(obj) => match obj.get("ignoreInside").and_then(|v| v.as_str()) {
        Some("at-rule" | "inside-at-rule") => true,
        _ => false,
      },
      None => false,
    };
    let ignore_local = match &secondary {
      Some(obj) => match obj.get("ignore").and_then(|v| v.as_str()) {
        Some("local") => true,
        _ => false,
      },
      None => false,
    };
    let custom_message = secondary
      .and_then(|obj| obj.get("message"))
      .and_then(|v| v.as_str())
      .map(|s| s.to_string());

    let mut diagnostics = Vec::new();

    for node in nodes {
      self.walk(
        node,
        &re,
        pattern_str,
        false, // not inside an at-rule at top level
        false, // not inside any block at top level
        &HashSet::new(),
        ignore_inside_at_rule,
        ignore_local,
        custom_message.as_deref(),
        &mut diagnostics,
      );
    }

    diagnostics
  }
}

impl ScssDollarVariablePattern {
  /// Recursively walk the AST, tracking at-rule context and loop variables.
  #[allow(clippy::too_many_arguments)]
  fn walk(
    &self,
    node: &CssNode,
    re: &Regex,
    pattern_str: &str,
    inside_at_rule: bool,
    inside_block: bool,
    loop_vars: &HashSet<String>,
    ignore_inside_at_rule: bool,
    ignore_local: bool,
    custom_message: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
  ) {
    match node {
      CssNode::Declaration(decl) => {
        if !decl.property.starts_with('$') {
          return;
        }

        let var_name = &decl.property[1..];
        if var_name.is_empty() {
          return;
        }

        // Skip the loop iterator variables themselves (e.g., $color in
        // `@each $color in $list`). Other variables declared inside loops
        // are still checked, matching Stylelint-scss behavior.
        if loop_vars.contains(var_name) {
          return;
        }

        // Skip all variables inside at-rules when ignoreInside is set
        if ignore_inside_at_rule && inside_at_rule {
          return;
        }

        // Skip local (non-top-level) variables when ignore: "local"
        if ignore_local && inside_block {
          return;
        }

        if !re.is_match(var_name) {
          let message = custom_message
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Expected $ variable name to match specified pattern".to_string());
          diagnostics.push(
            Diagnostic::new(self.name(), message)
              .severity(self.default_severity())
              .span(Span::new(decl.span.offset, decl.span.length)),
          );
        }
      }

      CssNode::AtRule(at_rule) => {
        // Extract loop variable names from @each and @for params.
        let mut child_loop_vars = loop_vars.clone();
        let name_lower = at_rule.name.to_lowercase();

        if name_lower == "each" {
          // @each $var1, $var2 in $list
          // params: "$var1, $var2 in $list"
          extract_each_vars(&at_rule.params, &mut child_loop_vars);
        } else if name_lower == "for" {
          // @for $i from 1 through 10
          // params: "$i from 1 through 10"
          extract_for_var(&at_rule.params, &mut child_loop_vars);
        }

        for child in &at_rule.children {
          self.walk(
            child,
            re,
            pattern_str,
            true, // inside an at-rule
            true, // inside a block
            &child_loop_vars,
            ignore_inside_at_rule,
            ignore_local,
            custom_message,
            diagnostics,
          );
        }
      }

      CssNode::Style(style_rule) => {
        // Declarations inside style rules
        for decl in &style_rule.declarations {
          let decl_node = CssNode::Declaration(decl.clone());
          self.walk(
            &decl_node,
            re,
            pattern_str,
            inside_at_rule,
            true, // inside a style rule block
            loop_vars,
            ignore_inside_at_rule,
            ignore_local,
            custom_message,
            diagnostics,
          );
        }
        // Nested style rules
        for child in &style_rule.children {
          let child_node = CssNode::Style(child.clone());
          self.walk(
            &child_node,
            re,
            pattern_str,
            inside_at_rule,
            true, // inside a style rule block
            loop_vars,
            ignore_inside_at_rule,
            ignore_local,
            custom_message,
            diagnostics,
          );
        }
      }

      CssNode::Comment(_) => {}
    }
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
  use gale_css_parser::{AtRule as ParserAtRule, Declaration, Span as ParserSpan, Syntax};

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn dollar_var(name: &str) -> CssNode {
    CssNode::Declaration(Declaration {
      property: name.to_string(),
      value: "red".to_string(),
      span: ParserSpan::new(0, 10),
      important: false,
    })
  }

  #[test]
  fn allows_kebab_case() {
    let nodes = vec![dollar_var("$my-var")];
    assert!(
      ScssDollarVariablePattern
        .check_root(&nodes, &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_non_matching_pattern() {
    let nodes = vec![dollar_var("$myVar")];
    let d = ScssDollarVariablePattern.check_root(&nodes, &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected $ variable name"));
  }

  #[test]
  fn skips_non_dollar_declarations() {
    let node = CssNode::Declaration(Declaration {
      property: "color".to_string(),
      value: "red".to_string(),
      span: ParserSpan::new(0, 10),
      important: false,
    });
    let nodes = vec![node];
    assert!(
      ScssDollarVariablePattern
        .check_root(&nodes, &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn skips_non_scss() {
    let css_ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    let nodes = vec![dollar_var("$myVar")];
    assert!(
      ScssDollarVariablePattern
        .check_root(&nodes, &css_ctx)
        .is_empty()
    );
  }

  // -- Loop variable tests --

  #[test]
  fn ignores_each_loop_variable() {
    // @each $breakpoint-name in $list { $breakpoint-name: "test"; }
    let nodes = vec![CssNode::AtRule(ParserAtRule {
      name: "each".to_string(),
      params: "$breakpoint-name in $grid-breakpoints".to_string(),
      span: ParserSpan::new(0, 80),
      children: vec![CssNode::Declaration(Declaration {
        property: "$breakpoint-name".to_string(),
        value: "test".to_string(),
        span: ParserSpan::new(50, 20),
        important: false,
      })],
    })];
    assert!(
      ScssDollarVariablePattern
        .check_root(&nodes, &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn ignores_each_loop_multiple_variables() {
    // @each $name, $value in $map { $name: ...; $value: ...; }
    let nodes = vec![CssNode::AtRule(ParserAtRule {
      name: "each".to_string(),
      params: "$name, $value in $map".to_string(),
      span: ParserSpan::new(0, 100),
      children: vec![
        CssNode::Declaration(Declaration {
          property: "$name".to_string(),
          value: "test".to_string(),
          span: ParserSpan::new(40, 15),
          important: false,
        }),
        CssNode::Declaration(Declaration {
          property: "$value".to_string(),
          value: "test".to_string(),
          span: ParserSpan::new(60, 15),
          important: false,
        }),
      ],
    })];
    assert!(
      ScssDollarVariablePattern
        .check_root(&nodes, &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn ignores_for_loop_variable() {
    // @for $i from 1 through 10 { $i: "override"; }
    let nodes = vec![CssNode::AtRule(ParserAtRule {
      name: "for".to_string(),
      params: "$i from 1 through 10".to_string(),
      span: ParserSpan::new(0, 60),
      children: vec![CssNode::Declaration(Declaration {
        property: "$i".to_string(),
        value: "override".to_string(),
        span: ParserSpan::new(30, 15),
        important: false,
      })],
    })];
    assert!(
      ScssDollarVariablePattern
        .check_root(&nodes, &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_non_loop_vars_inside_each_loop() {
    // @each $name in $list { $otherBad: "test"; }
    // $otherBad is NOT the loop iterator — should still be checked.
    let nodes = vec![CssNode::AtRule(ParserAtRule {
      name: "each".to_string(),
      params: "$name in $list".to_string(),
      span: ParserSpan::new(0, 60),
      children: vec![CssNode::Declaration(Declaration {
        property: "$otherBad".to_string(),
        value: "test".to_string(),
        span: ParserSpan::new(30, 15),
        important: false,
      })],
    })];
    let d = ScssDollarVariablePattern.check_root(&nodes, &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected $ variable name"));
  }

  // -- ignoreInside tests --

  #[test]
  fn ignore_inside_at_rule_skips_all_vars_in_at_rules() {
    use serde_json::json;
    let opts = json!(["^pf-v[56]-", {"ignoreInside": "at-rule"}]);
    let ctx = RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: Some(&opts),
    };

    // Variable inside a @mixin — should be ignored
    let nodes = vec![CssNode::AtRule(ParserAtRule {
      name: "mixin".to_string(),
      params: "my-mixin".to_string(),
      span: ParserSpan::new(0, 60),
      children: vec![CssNode::Declaration(Declaration {
        property: "$local-var".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(30, 15),
        important: false,
      })],
    })];
    assert!(
      ScssDollarVariablePattern
        .check_root(&nodes, &ctx)
        .is_empty()
    );
  }

  #[test]
  fn ignore_inside_at_rule_still_reports_top_level() {
    use serde_json::json;
    let opts = json!(["^pf-v[56]-", {"ignoreInside": "at-rule"}]);
    let ctx = RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: Some(&opts),
    };

    // Top-level variable that doesn't match — should still be reported
    let nodes = vec![dollar_var("$bad-name")];
    let d = ScssDollarVariablePattern.check_root(&nodes, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected $ variable name"));
  }

  #[test]
  fn ignore_inside_inside_at_rule_synonym() {
    use serde_json::json;
    let opts = json!(["^pf-", {"ignoreInside": "inside-at-rule"}]);
    let ctx = RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: Some(&opts),
    };

    let nodes = vec![CssNode::AtRule(ParserAtRule {
      name: "function".to_string(),
      params: "my-fn()".to_string(),
      span: ParserSpan::new(0, 60),
      children: vec![CssNode::Declaration(Declaration {
        property: "$local".to_string(),
        value: "1".to_string(),
        span: ParserSpan::new(30, 10),
        important: false,
      })],
    })];
    assert!(
      ScssDollarVariablePattern
        .check_root(&nodes, &ctx)
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
    // @each $breakpoint, $breakpoint-value in $map {
    //   $breakpoint-name: "";           <- local var, not iterator
    //   $breakpoint: "md";              <- reassignment of iterator
    // }
    let nodes = vec![CssNode::AtRule(ParserAtRule {
      name: "each".to_string(),
      params: "$breakpoint, $breakpoint-value in $pf-v6-map".to_string(),
      span: ParserSpan::new(0, 120),
      children: vec![
        CssNode::Declaration(Declaration {
          property: "$breakpoint-name".to_string(),
          value: "\"\"".to_string(),
          span: ParserSpan::new(60, 20),
          important: false,
        }),
        CssNode::Declaration(Declaration {
          property: "$breakpoint".to_string(),
          value: "\"md\"".to_string(),
          span: ParserSpan::new(85, 18),
          important: false,
        }),
      ],
    })];
    assert!(
      ScssDollarVariablePattern
        .check_root(&nodes, &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn ignores_vars_in_nested_at_rule_inside_each() {
    // @each $name in $list {
    //   @if $name != "base" {
    //     $breakpoint-name: -on-#{$name};
    //   }
    // }
    let nodes = vec![CssNode::AtRule(ParserAtRule {
      name: "each".to_string(),
      params: "$name in $list".to_string(),
      span: ParserSpan::new(0, 120),
      children: vec![CssNode::AtRule(ParserAtRule {
        name: "if".to_string(),
        params: "$name != \"base\"".to_string(),
        span: ParserSpan::new(30, 60),
        children: vec![CssNode::Declaration(Declaration {
          property: "$breakpoint-name".to_string(),
          value: "-on-#{$name}".to_string(),
          span: ParserSpan::new(50, 30),
          important: false,
        })],
      })],
    })];
    assert!(
      ScssDollarVariablePattern
        .check_root(&nodes, &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn ignores_vars_inside_for_loop() {
    // @for $i from 1 through 10 {
    //   $width: $i * 10%;
    // }
    let nodes = vec![CssNode::AtRule(ParserAtRule {
      name: "for".to_string(),
      params: "$i from 1 through 10".to_string(),
      span: ParserSpan::new(0, 80),
      children: vec![CssNode::Declaration(Declaration {
        property: "$width".to_string(),
        value: "$i * 10%".to_string(),
        span: ParserSpan::new(40, 18),
        important: false,
      })],
    })];
    assert!(
      ScssDollarVariablePattern
        .check_root(&nodes, &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn still_reports_non_matching_vars_inside_and_outside_loop() {
    // $myBadVar: red;  (top-level, camelCase, doesn't match default kebab pattern)
    // @each $name in $list { $localVar: "test"; }  ($localVar also doesn't match)
    let nodes = vec![
      dollar_var("$myBadVar"),
      CssNode::AtRule(ParserAtRule {
        name: "each".to_string(),
        params: "$name in $list".to_string(),
        span: ParserSpan::new(20, 60),
        children: vec![CssNode::Declaration(Declaration {
          property: "$localVar".to_string(),
          value: "test".to_string(),
          span: ParserSpan::new(50, 15),
          important: false,
        })],
      }),
    ];
    let d = ScssDollarVariablePattern.check_root(&nodes, &scss_ctx());
    // Both $myBadVar and $localVar should be reported (neither is a loop iterator)
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn reports_non_matching_var_inside_mixin_without_ignore_option() {
    // Without ignoreInside, vars in @mixin (non-loop) should still be checked
    let nodes = vec![CssNode::AtRule(ParserAtRule {
      name: "mixin".to_string(),
      params: "my-mixin".to_string(),
      span: ParserSpan::new(0, 60),
      children: vec![CssNode::Declaration(Declaration {
        property: "$myBadVar".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(30, 15),
        important: false,
      })],
    })];
    let d = ScssDollarVariablePattern.check_root(&nodes, &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected $ variable name"));
  }
}
