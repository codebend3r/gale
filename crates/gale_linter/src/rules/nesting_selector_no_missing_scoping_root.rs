use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Ensure that a nested rule using `&` always has a valid scoping root.
///
/// In CSS nesting, `&` refers to the parent selector. Using `&` in a
/// top-level rule is invalid because there is no parent context. This rule
/// flags style rules at the document root that contain `&` in their selector.
///
/// Equivalent to Stylelint's `nesting-selector-no-missing-scoping-root` rule.
pub struct NestingSelectorNoMissingScopingRoot;

/// Check if a selector contains the nesting selector `&`.
///
/// Skips occurrences inside strings and attribute selectors to avoid false
/// positives.
fn selector_contains_nesting(selector: &str) -> bool {
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    match chars[i] {
      // Skip attribute selectors
      '[' => {
        let mut depth = 1;
        i += 1;
        while i < len && depth > 0 {
          match chars[i] {
            '[' => depth += 1,
            ']' => depth -= 1,
            '"' | '\'' => {
              let quote = chars[i];
              i += 1;
              while i < len && chars[i] != quote {
                if chars[i] == '\\' {
                  i += 1;
                }
                i += 1;
              }
            }
            _ => {}
          }
          i += 1;
        }
      }
      // Skip strings
      '"' | '\'' => {
        let quote = chars[i];
        i += 1;
        while i < len && chars[i] != quote {
          if chars[i] == '\\' {
            i += 1;
          }
          i += 1;
        }
        i += 1;
      }
      '&' => return true,
      _ => i += 1,
    }
  }

  false
}

/// Parse the `ignoreAtRules` list from the rule's secondary options.
///
/// Supports both `[true, { ignoreAtRules: ["mixin"] }]` and
/// `{ ignoreAtRules: ["mixin"] }` shapes.
fn parse_ignore_at_rules(options: Option<&serde_json::Value>) -> Vec<String> {
  let Some(opts) = options else {
    return vec![];
  };
  let obj = match opts {
    serde_json::Value::Object(o) => o,
    serde_json::Value::Array(arr) => {
      for item in arr {
        if let serde_json::Value::Object(o) = item {
          if let Some(serde_json::Value::Array(names)) = o.get("ignoreAtRules") {
            return names
              .iter()
              .filter_map(|v| v.as_str().map(String::from))
              .collect();
          }
        }
      }
      return vec![];
    }
    _ => return vec![],
  };
  if let Some(serde_json::Value::Array(names)) = obj.get("ignoreAtRules") {
    names
      .iter()
      .filter_map(|v| v.as_str().map(String::from))
      .collect()
  } else {
    vec![]
  }
}

impl Rule for NestingSelectorNoMissingScopingRoot {
  fn name(&self) -> &'static str {
    "nesting-selector-no-missing-scoping-root"
  }

  fn description(&self) -> &'static str {
    "Disallow nesting selector & without a scoping root"
  }

  fn default_severity(&self) -> Severity {
    Severity::Error
  }

  /// Walk the top-level nodes. Any top-level style rule whose selector
  /// contains `&` is invalid — there is no parent to scope against.
  /// Nested style rules inside other style rules are fine.
  fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let ignore_at_rules = parse_ignore_at_rules(ctx.options);
    let mut diags = Vec::new();
    for node in nodes {
      if let CssNode::Style(rule) = node
        && selector_contains_nesting(&rule.selector)
      {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!(
              "Unexpected nesting selector \"&\" without a scoping root in \"{}\"",
              rule.selector
            ),
          )
          .severity(self.default_severity())
          .span(Span::new(rule.span.offset, rule.span.length)),
        );
      }
      // Also check inside @-rule children (e.g. top-level styles inside @layer),
      // unless the at-rule name is in the ignoreAtRules list.
      if let CssNode::AtRule(at) = node {
        let at_rule_ignored = ignore_at_rules
          .iter()
          .any(|ignored| ignored.eq_ignore_ascii_case(&at.name));
        if at_rule_ignored {
          continue;
        }
        for child in &at.children {
          if let CssNode::Style(rule) = child
            && selector_contains_nesting(&rule.selector)
          {
            diags.push(
              Diagnostic::new(
                self.name(),
                format!(
                  "Unexpected nesting selector \"&\" without a scoping root in \"{}\"",
                  rule.selector
                ),
              )
              .severity(self.default_severity())
              .span(Span::new(rule.span.offset, rule.span.length)),
            );
          }
        }
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, CssNode, Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn ctx_with_options(opts: &'static serde_json::Value) -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: Some(opts),
    }
  }

  fn style(sel: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  fn nested_style(parent_sel: &str, child_sel: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: parent_sel.to_string(),
      declarations: vec![],
      children: vec![StyleRule {
        selector: child_sel.to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "blue".to_string(),
          span: ParserSpan::new(0, 0),
          important: false,
        }],
        span: ParserSpan::new(0, 0),
        ..Default::default()
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_top_level_nesting_selector() {
    let nodes = vec![style("& .child")];
    let d = NestingSelectorNoMissingScopingRoot.check_root(&nodes, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("&"));
  }

  #[test]
  fn reports_top_level_ampersand_alone() {
    let nodes = vec![style("&")];
    let d = NestingSelectorNoMissingScopingRoot.check_root(&nodes, &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_normal_top_level_selector() {
    let nodes = vec![style(".parent")];
    let d = NestingSelectorNoMissingScopingRoot.check_root(&nodes, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_nested_nesting_selector() {
    // A nested rule using & inside a parent is fine; check_root only
    // sees the top-level node ".parent" which does not contain &.
    let nodes = vec![nested_style(".parent", "& .child")];
    let d = NestingSelectorNoMissingScopingRoot.check_root(&nodes, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_inside_at_rule_without_parent() {
    // A style rule with & directly inside @layer (no parent style rule)
    let nodes = vec![CssNode::AtRule(AtRule {
      name: "layer".to_string(),
      params: "utilities".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![style("& .child")],
    })];
    let d = NestingSelectorNoMissingScopingRoot.check_root(&nodes, &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_no_ampersand_inside_at_rule() {
    let nodes = vec![CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "screen".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![style(".normal")],
    })];
    let d = NestingSelectorNoMissingScopingRoot.check_root(&nodes, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_at_rule_when_listed_in_ignore_at_rules() {
    // [true, { ignoreAtRules: ["mixin"] }] — nesting inside @mixin should
    // be silenced.
    static OPTS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let opts = OPTS.get_or_init(|| serde_json::json!([true, { "ignoreAtRules": ["mixin"] }]));
    let nodes = vec![CssNode::AtRule(AtRule {
      name: "mixin".to_string(),
      params: "onebox-favicon($class, $image)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![style("&.#{$class} .source")],
    })];
    let d = NestingSelectorNoMissingScopingRoot.check_root(&nodes, &ctx_with_options(opts));
    assert!(d.is_empty(), "expected no diagnostics, got: {:?}", d);
  }

  #[test]
  fn still_reports_non_ignored_at_rule() {
    // @layer is not in the ignore list — should still fire.
    static OPTS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let opts = OPTS.get_or_init(|| serde_json::json!([true, { "ignoreAtRules": ["mixin"] }]));
    let nodes = vec![CssNode::AtRule(AtRule {
      name: "layer".to_string(),
      params: "utilities".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![style("& .child")],
    })];
    let d = NestingSelectorNoMissingScopingRoot.check_root(&nodes, &ctx_with_options(opts));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn ignore_at_rules_is_case_insensitive() {
    // Option says "mixin" (lowercase), at-rule name "Mixin" — should be ignored.
    static OPTS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let opts = OPTS.get_or_init(|| serde_json::json!([true, { "ignoreAtRules": ["mixin"] }]));
    let nodes = vec![CssNode::AtRule(AtRule {
      name: "Mixin".to_string(),
      params: "foo".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![style("& .bar")],
    })];
    let d = NestingSelectorNoMissingScopingRoot.check_root(&nodes, &ctx_with_options(opts));
    assert!(
      d.is_empty(),
      "case-insensitive match should suppress diagnostic"
    );
  }
}
