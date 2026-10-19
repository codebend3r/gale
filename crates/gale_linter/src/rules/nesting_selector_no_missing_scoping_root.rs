use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "nesting-selector-no-missing-scoping-root";

declare_rule! {
  /// Ensure that a nested rule using `&` always has a valid scoping root.
  ///
  /// In CSS nesting, `&` refers to the parent selector. Using `&` in a
  /// top-level rule is invalid because there is no parent context. This rule
  /// flags style rules at the document root that contain `&` in their selector.
  ///
  /// Equivalent to Stylelint's `nesting-selector-no-missing-scoping-root` rule.
  pub struct NestingSelectorNoMissingScopingRoot => NestingSelectorNoMissingScopingRootCheck {
    name: NAME,
    description: "Disallow nesting selector & without a scoping root",
    default_severity: Error,
  }
}

/// `nesting-selector-no-missing-scoping-root` as configured.
pub struct NestingSelectorNoMissingScopingRootCheck {
  /// `ignoreAtRules`: at-rule names (matched in any case) and `/regex/`
  /// entries.
  ignore_at_rules: Matchers,
}

impl NestingSelectorNoMissingScopingRootCheck {
  /// Stylelint's options: `true`, then `ignoreAtRules`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.primary_true();
    let ignore_at_rules = Matchers::ignoring_case(&options.strings("ignoreAtRules"));
    options.finish()?;
    Ok(Self { ignore_at_rules })
  }

  /// Whether the at-rule `name` is one of `ignoreAtRules`: a `/regex/`
  /// entry matches by search, a name in any case.
  fn is_ignored_at_rule(&self, name: &str) -> bool {
    self.ignore_at_rules.matches(name)
  }
}

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

impl Check for NestingSelectorNoMissingScopingRootCheck {
  /// Walk the top-level nodes. Any top-level style rule whose selector
  /// contains `&` is invalid — there is no parent to scope against.
  /// Nested style rules inside other style rules are fine.
  fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for node in nodes {
      if let CssNode::Style(rule) = node
        && selector_contains_nesting(&rule.selector)
      {
        diags.push(
          Diagnostic::new(
            NAME,
            format!(
              "Unexpected nesting selector \"&\" without a scoping root in \"{}\"",
              rule.selector
            ),
          )
          .span(Span::new(rule.span.offset, rule.span.length)),
        );
      }
      // Also check inside @-rule children (e.g. top-level styles inside @layer),
      // unless the at-rule name is in the ignoreAtRules list.
      if let CssNode::AtRule(at) = node {
        if self.is_ignored_at_rule(&at.name) {
          continue;
        }
        for child in &at.children {
          if let CssNode::Style(rule) = child
            && selector_contains_nesting(&rule.selector)
          {
            diags.push(
              Diagnostic::new(
                NAME,
                format!(
                  "Unexpected nesting selector \"&\" without a scoping root in \"{}\"",
                  rule.selector
                ),
              )
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
  use gale_css_parser::{AtRule, CssNode, Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx, scss_ctx};

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
    let rule = configured(&NestingSelectorNoMissingScopingRoot, None);
    let nodes = vec![style("& .child")];
    let d = rule.check_root(&nodes, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("&"));
  }

  #[test]
  fn reports_top_level_ampersand_alone() {
    let rule = configured(&NestingSelectorNoMissingScopingRoot, None);
    let nodes = vec![style("&")];
    let d = rule.check_root(&nodes, &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_normal_top_level_selector() {
    let rule = configured(&NestingSelectorNoMissingScopingRoot, None);
    let nodes = vec![style(".parent")];
    let d = rule.check_root(&nodes, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_nested_nesting_selector() {
    let rule = configured(&NestingSelectorNoMissingScopingRoot, None);
    // A nested rule using & inside a parent is fine; check_root only
    // sees the top-level node ".parent" which does not contain &.
    let nodes = vec![nested_style(".parent", "& .child")];
    let d = rule.check_root(&nodes, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_inside_at_rule_without_parent() {
    let rule = configured(&NestingSelectorNoMissingScopingRoot, None);
    // A style rule with & directly inside @layer (no parent style rule)
    let nodes = vec![CssNode::AtRule(AtRule {
      name: "layer".to_string(),
      params: "utilities".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![style("& .child")],
    })];
    let d = rule.check_root(&nodes, &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_no_ampersand_inside_at_rule() {
    let rule = configured(&NestingSelectorNoMissingScopingRoot, None);
    let nodes = vec![CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "screen".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![style(".normal")],
    })];
    let d = rule.check_root(&nodes, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_at_rule_when_listed_in_ignore_at_rules() {
    // [true, { ignoreAtRules: ["mixin"] }] — nesting inside @mixin should
    // be silenced.
    static OPTS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let opts = OPTS.get_or_init(|| serde_json::json!([true, { "ignoreAtRules": ["mixin"] }]));
    let rule = configured(&NestingSelectorNoMissingScopingRoot, opts.clone());
    let nodes = vec![CssNode::AtRule(AtRule {
      name: "mixin".to_string(),
      params: "onebox-favicon($class, $image)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![style("&.#{$class} .source")],
    })];
    let d = rule.check_root(&nodes, &scss_ctx());
    assert!(d.is_empty(), "expected no diagnostics, got: {:?}", d);
  }

  #[test]
  fn still_reports_non_ignored_at_rule() {
    // @layer is not in the ignore list — should still fire.
    static OPTS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let opts = OPTS.get_or_init(|| serde_json::json!([true, { "ignoreAtRules": ["mixin"] }]));
    let rule = configured(&NestingSelectorNoMissingScopingRoot, opts.clone());
    let nodes = vec![CssNode::AtRule(AtRule {
      name: "layer".to_string(),
      params: "utilities".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![style("& .child")],
    })];
    let d = rule.check_root(&nodes, &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn ignore_at_rules_is_case_insensitive() {
    // Option says "mixin" (lowercase), at-rule name "Mixin" — should be ignored.
    static OPTS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let opts = OPTS.get_or_init(|| serde_json::json!([true, { "ignoreAtRules": ["mixin"] }]));
    let rule = configured(&NestingSelectorNoMissingScopingRoot, opts.clone());
    let nodes = vec![CssNode::AtRule(AtRule {
      name: "Mixin".to_string(),
      params: "foo".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![style("& .bar")],
    })];
    let d = rule.check_root(&nodes, &scss_ctx());
    assert!(
      d.is_empty(),
      "case-insensitive match should suppress diagnostic"
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreAtRules": ["mixin", "/^incl/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Invalid option value \"always\" for rule \"nesting-selector-no-missing-scoping-root\""]
    );
  }

  #[test]
  fn ignore_at_rules_takes_regexes() {
    let rule = configured(
      &NestingSelectorNoMissingScopingRoot,
      serde_json::json!([true, { "ignoreAtRules": ["/^mix/"] }]),
    );
    let nodes = vec![CssNode::AtRule(AtRule {
      name: "mixin".to_string(),
      params: "foo".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![style("& .bar")],
    })];
    assert!(rule.check_root(&nodes, &ctx()).is_empty());
  }
}
