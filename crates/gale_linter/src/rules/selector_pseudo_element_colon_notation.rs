use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::selector::postcss::{self, Kind};
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::data::is_legacy_pseudo_element;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::stylelint_version::stylelint_major_version;

const NAME: &str = "selector-pseudo-element-colon-notation";

declare_rule! {
  /// Enforces a specific colon notation (`::` or `:`) for pseudo-elements that
  /// support both syntaxes (`:before`, `:after`, `:first-line`, `:first-letter`).
  ///
  /// Equivalent to Stylelint's `selector-pseudo-element-colon-notation` rule.
  /// Primary option: `"double"` (default) or `"single"`.  The fix rewrites the
  /// colons and keeps the name as written.
  pub struct SelectorPseudoElementColonNotation => SelectorPseudoElementColonNotationCheck {
    name: NAME,
    description: "Specify single or double colon notation for applicable pseudo-elements",
    default_severity: Warning,
  }
}

/// `selector-pseudo-element-colon-notation` as configured.
pub struct SelectorPseudoElementColonNotationCheck {
  /// Whether the four legacy pseudo-elements take a single colon.
  single: bool,
}

impl SelectorPseudoElementColonNotationCheck {
  /// Stylelint's options: `"single"` or `"double"`.  Gale reads no option as `"double"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&["single", "double"]);
    options.finish()?;
    Ok(Self {
      single: option.unwrap_or("double") == "single",
    })
  }
}

impl Check for SelectorPseudoElementColonNotationCheck {
  /// Flags the four dual-syntax pseudo-elements written with the colon
  /// notation the option forbids, in every style rule's selector as written.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let single = self.single;
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
    for rule in tree
      .walk_rules()
      .filter(|rule| rule.is_standard_syntax() && !rule.in_keyframes())
    {
      let prelude = rule.raw_selector();
      let first = diags.len();
      if !prelude.contains(':') {
        continue;
      }
      let Some(selectors) = postcss::parse(&prelude, rule.selector_start()) else {
        continue;
      };
      postcss::walk(&selectors, &mut |visit| {
        let node = visit.node;
        if node.kind != Kind::Pseudo {
          return;
        }
        let name = node.value.trim_start_matches(':');
        if !is_legacy_pseudo_element(name) {
          return;
        }
        let written = node.value.len() - name.len();
        let is_double = node.value.starts_with("::");
        if is_double != single {
          return;
        }
        diags.push(
          Diagnostic::new(NAME, message)
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
      if !rule.selector_fixable() {
        diags[first..].iter_mut().for_each(|diag| diag.fix = None);
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;

  use crate::testing::{fix, invalid_options, lint};

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

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(RULE, serde_json::json!("single")).is_empty());
    assert_eq!(
      invalid_options(RULE, serde_json::json!("triple")),
      ["Invalid option value \"triple\" for rule \"selector-pseudo-element-colon-notation\""]
    );
  }
}
