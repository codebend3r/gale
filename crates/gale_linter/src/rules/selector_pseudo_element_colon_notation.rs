use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};
use crate::selector::postcss::{self, Kind};
use crate::standard_syntax::is_standard_syntax_selector;
use crate::style_rules::scan_style_rules;

/// Enforces a specific colon notation (`::` or `:`) for pseudo-elements that
/// support both syntaxes (`:before`, `:after`, `:first-line`, `:first-letter`).
///
/// Equivalent to Stylelint's `selector-pseudo-element-colon-notation` rule.
/// Primary option: `"double"` (default) or `"single"`.  The fix rewrites the
/// colons and keeps the name as written.
pub struct SelectorPseudoElementColonNotation;

/// Stylelint's `levelOneAndTwoPseudoElements`.
const LEGACY_PSEUDO_ELEMENTS: &[&str] = &["before", "after", "first-line", "first-letter"];

impl Rule for SelectorPseudoElementColonNotation {
  fn name(&self) -> &'static str {
    "selector-pseudo-element-colon-notation"
  }

  fn description(&self) -> &'static str {
    "Specify single or double colon notation for applicable pseudo-elements"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags the four dual-syntax pseudo-elements written with the colon
  /// notation the option forbids, in every style rule's selector as written.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let single = ctx.primary_option_str() == Some("single");
    let (colons, message) = if single {
      (":", "Expected single colon pseudo-element notation")
    } else {
      ("::", "Expected double colon pseudo-element notation")
    };

    let mut diags = Vec::new();
    for rule in scan_style_rules(ctx.source, ctx.syntax) {
      if !rule.prelude.contains(':') || !is_standard_syntax_selector(&rule.prelude) {
        continue;
      }
      let Some(selectors) = postcss::parse(&rule.prelude, rule.offset) else {
        continue;
      };
      postcss::walk(&selectors, &mut |visit| {
        let node = visit.node;
        if node.kind != Kind::Pseudo {
          return;
        }
        let name = node.value.trim_start_matches(':');
        if !LEGACY_PSEUDO_ELEMENTS.contains(&name.to_ascii_lowercase().as_str()) {
          return;
        }
        let written = node.value.len() - name.len();
        let is_double = node.value.starts_with("::");
        if is_double != single {
          return;
        }
        diags.push(
          Diagnostic::new(self.name(), message)
            .severity(self.default_severity())
            .span(Span::new(node.start, if is_double { 2 } else { 1 }))
            .fix(Fix::new(
              format!("Write \"{colons}{name}\""),
              vec![Edit::new(Span::new(node.start, written), colons)],
            )),
        );
      });
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use gale_css_parser::Syntax;
  use gale_diagnostics::apply_fixes;

  use crate::{LintRunner, RuleRegistry};

  /// Lint `css` with only this rule enabled, configured with `options`.
  fn lint(css: &str, options: serde_json::Value) -> Vec<gale_diagnostics::Diagnostic> {
    let rule = "selector-pseudo-element-colon-notation".to_string();
    let runner = LintRunner::with_options(
      RuleRegistry::default(),
      vec![rule.clone()],
      HashMap::from([(rule, options)]),
    );
    runner.lint_source(css, "test.css", Syntax::Css).diagnostics
  }

  /// `css` after applying the rule's fixes until nothing changes, the way
  /// `gale --fix` does.
  fn fix(css: &str, options: serde_json::Value) -> String {
    let mut current = css.to_string();
    for _ in 0..10 {
      let (next, applied) = apply_fixes(&current, &lint(&current, options.clone()));
      if applied == 0 || next == current {
        break;
      }
      current = next;
    }
    current
  }

  #[test]
  fn single_keeps_the_name_as_written() {
    let single = serde_json::json!("single");
    assert_eq!(fix("a::bEfOrE { }", single.clone()), "a:bEfOrE { }");
    assert_eq!(
      fix("a::before, a::after, a::first-letter { }", single.clone()),
      "a:before, a:after, a:first-letter { }"
    );
    assert_eq!(
      fix("a\\:before-none::before { }", single.clone()),
      "a\\:before-none:before { }"
    );
    let warnings = lint("a::before { }", single);
    assert_eq!(warnings.len(), 1);
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (1, 2));
  }

  #[test]
  fn double_adds_a_colon() {
    let double = serde_json::json!("double");
    assert_eq!(
      fix("a:before, a:after, a:FIRST-LINE { }", double.clone()),
      "a::before, a::after, a::FIRST-LINE { }"
    );
    for css in [
      "a::before { }",
      "::selection { }",
      "a[data-before=':before'] { }",
      "li::marker { }",
    ] {
      assert!(lint(css, double.clone()).is_empty(), "{css}");
    }
  }

  #[test]
  fn reads_selectors_between_comments() {
    let css = "/* a */\na::after, /* b */\na::after\n{}";
    assert_eq!(
      fix(css, serde_json::json!("single")),
      "/* a */\na:after, /* b */\na:after\n{}"
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!(["single", { "disableFix": true }]);
    assert_eq!(lint("a::after { }", options.clone()).len(), 1);
    assert_eq!(fix("a::after { }", options), "a::after { }");
  }
}
