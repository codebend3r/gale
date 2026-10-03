use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};
use crate::selector::postcss::{self, Kind};
use crate::standard_syntax::is_standard_syntax_selector;
use crate::stylelint_version::stylelint_major_version;

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

    // Stylelint 13 and older searched the selector text for `:before` and
    // the like, so a `::before` it wanted single was reported from its
    // second colon; 14 reports from the first.
    let second_colon = single && stylelint_major_version() <= 13;

    let mut diags = Vec::new();
    for rule in &ctx.scanned_rules().style_rules {
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
            .span(if is_double && second_colon {
              Span::new(node.start + 1, 1)
            } else {
              Span::new(node.start, if is_double { 2 } else { 1 })
            })
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
  use gale_css_parser::Syntax;

  use crate::testing::{fix, lint};

  const RULE: &str = "selector-pseudo-element-colon-notation";

  #[test]
  fn single_keeps_the_name_as_written() {
    let single = serde_json::json!("single");
    assert_eq!(
      fix(RULE, single.clone(), "a::bEfOrE { }", Syntax::Css),
      "a:bEfOrE { }"
    );
    assert_eq!(
      fix(
        RULE,
        single.clone(),
        "a::before, a::after, a::first-letter { }",
        Syntax::Css
      ),
      "a:before, a:after, a:first-letter { }"
    );
    assert_eq!(
      fix(
        RULE,
        single.clone(),
        "a\\:before-none::before { }",
        Syntax::Css
      ),
      "a\\:before-none:before { }"
    );
    let warnings = lint(RULE, single, "a::before { }", Syntax::Css);
    assert_eq!(warnings.len(), 1);
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (1, 2));
  }

  #[test]
  fn double_adds_a_colon() {
    let double = serde_json::json!("double");
    assert_eq!(
      fix(
        RULE,
        double.clone(),
        "a:before, a:after, a:FIRST-LINE { }",
        Syntax::Css
      ),
      "a::before, a::after, a::FIRST-LINE { }"
    );
    for css in [
      "a::before { }",
      "::selection { }",
      "a[data-before=':before'] { }",
      "li::marker { }",
    ] {
      assert!(
        lint(RULE, double.clone(), css, Syntax::Css).is_empty(),
        "{css}"
      );
    }
  }

  #[test]
  fn reads_selectors_between_comments() {
    let css = "/* a */\na::after, /* b */\na::after\n{}";
    assert_eq!(
      fix(RULE, serde_json::json!("single"), css, Syntax::Css),
      "/* a */\na:after, /* b */\na:after\n{}"
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!(["single", { "disableFix": true }]);
    assert_eq!(
      lint(RULE, options.clone(), "a::after { }", Syntax::Css).len(),
      1
    );
    assert_eq!(
      fix(RULE, options, "a::after { }", Syntax::Css),
      "a::after { }"
    );
  }
}
