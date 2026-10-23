use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::value_parser::{self, NodeKind, ValueNode};
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::chars::is_word_byte;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::standard_syntax::is_standard_syntax_value;

const NAME: &str = "hue-degree-notation";

declare_rule! {
  /// Specify number or angle notation for degree hues.
  ///
  /// Equivalent to Stylelint's `hue-degree-notation` rule, including its
  /// autofix.  Checks the hue of `hsl()`, `hsla()` and `hwb()` (first channel)
  /// and `lch()` and `oklch()` (third channel), also in relative color syntax.
  /// Primary option: `"angle"` (hues as `120deg`) or `"number"` (hues as
  /// `120`).
  pub struct HueDegreeNotation => HueDegreeNotationCheck {
    name: NAME,
    description: "Specify number or angle notation for degree hues",
    default_severity: Warning,
  }
}

/// `hue-degree-notation` as configured.
pub struct HueDegreeNotationCheck {
  /// `Some(true)` for `"angle"`, `Some(false)` for `"number"`; the rule
  /// checks nothing without one.
  want_angle: Option<bool>,
}

impl HueDegreeNotationCheck {
  /// Stylelint's options: `"angle"` or `"number"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let notation = options.primary_one_of(&["angle", "number"]);
    options.finish()?;
    Ok(Self {
      want_angle: notation.map(|notation| notation == "angle"),
    })
  }
}

/// Functions whose first channel is the hue.
const HUE_FIRST: &[&str] = &["hsl", "hsla", "hwb"];

/// Functions whose third channel is the hue.
const HUE_THIRD: &[&str] = &["lch", "oklch"];

impl Check for HueDegreeNotationCheck {
  /// Reports every degree or unitless hue in the wrong notation, with a fix
  /// that adds or drops the `deg` unit.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let Some(want_angle) = self.want_angle else {
      return Vec::new();
    };
    let mut diags = Vec::new();

    for decl in tree.walk_decls() {
      let value = decl.written_value();
      if !mentions_hue_function(value) {
        continue;
      }
      let parsed = value_parser::parse(value);
      let mut check = |node: &ValueNode<'_>| {
        if node.kind != NodeKind::Function {
          return;
        }
        let Some(hue) = find_hue(node) else { return };
        let unfixed = hue.value;
        if !is_standard_syntax_value(unfixed) {
          return;
        }
        let Some((number, unit)) = value_parser::unit(unfixed) else {
          return;
        };
        let is_degree = unit.eq_ignore_ascii_case("deg");
        let is_number = unit.is_empty();
        if !(is_degree || is_number) || (want_angle && is_degree) || (!want_angle && is_number) {
          return;
        }
        let fixed = if want_angle {
          format!("{number}deg")
        } else {
          number.to_string()
        };
        let start = decl.value_span.start + hue.source_index;
        let span = Span::new(start, hue.source_end_index - hue.source_index);
        diags.push(
          Diagnostic::new(NAME, format!("Expected \"{unfixed}\" to be \"{fixed}\""))
            .span(span)
            .fix(Fix::new(
              format!("Replace \"{unfixed}\" with \"{fixed}\""),
              vec![Edit::new(span, fixed.clone())],
            )),
        );
      };
      value_parser::walk(&parsed, &mut |node| {
        check(node);
        true
      });
    }
    diags
  }
}

/// Whether `value` calls a hue color function (Stylelint tests
/// `/\b(?:hsl|hsla|hwb|lch|oklch)\(/i`).
fn mentions_hue_function(value: &str) -> bool {
  let lower = value.to_ascii_lowercase();
  lower.match_indices('(').any(|(paren, _)| {
    HUE_FIRST.iter().chain(HUE_THIRD).any(|name| {
      lower[..paren].ends_with(name) && {
        let start = paren - name.len();
        start == 0 || !is_word_byte(lower.as_bytes()[start - 1])
      }
    })
  })
}

/// The hue argument of a hue color function, skipping `from <color>` in
/// relative color syntax.
fn find_hue<'n, 'a>(node: &'n ValueNode<'a>) -> Option<&'n ValueNode<'a>> {
  let name = node.value.to_ascii_lowercase();
  let args: Vec<&ValueNode<'a>> = node
    .nodes
    .iter()
    .filter(|n| n.kind == NodeKind::Word || n.kind == NodeKind::Function)
    .collect();
  let offset = if args
    .first()
    .is_some_and(|a| a.value.eq_ignore_ascii_case("from"))
  {
    2
  } else {
    0
  };
  if HUE_FIRST.contains(&name.as_str()) {
    args.get(offset).copied()
  } else if HUE_THIRD.contains(&name.as_str()) {
    args.get(2 + offset).copied()
  } else {
    None
  }
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, lint};

  const RULE: &str = "hue-degree-notation";

  #[test]
  fn adds_the_deg_unit() {
    let angle = || json!(["angle"]);
    assert_eq!(
      fix(RULE, angle(), "a { color: hsl(120 60% 70%) }", Syntax::Css),
      "a { color: hsl(120deg 60% 70%) }"
    );
    assert_eq!(
      fix(
        RULE,
        angle(),
        "a { color: oklch(from red l c 120.5) }",
        Syntax::Css
      ),
      "a { color: oklch(from red l c 120.5deg) }"
    );
    assert_eq!(
      fix(
        RULE,
        angle(),
        "a { color: hsl(/*c*/120 60% 70%) }",
        Syntax::Css
      ),
      "a { color: hsl(/*c*/120deg 60% 70%) }"
    );
  }

  #[test]
  fn drops_the_deg_unit_in_any_case() {
    assert_eq!(
      fix(
        RULE,
        json!(["number"]),
        "a { color: LCH(56.29% 19.86 10DEG) }",
        Syntax::Css
      ),
      "a { color: LCH(56.29% 19.86 10) }"
    );
  }

  #[test]
  fn ignores_variables_and_other_units() {
    let angle = || json!(["angle"]);
    assert!(lint(RULE, angle(), "a { color: hsl($h 60% 70%) }", Syntax::Scss).is_empty());
    assert!(
      lint(
        RULE,
        angle(),
        "a { color: hsl(1turn 60% 70%) }",
        Syntax::Css
      )
      .is_empty()
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = json!(["angle", { "disableFix": true }]);
    let source = "a { color: hsl(120 60% 70%) }";
    assert_eq!(fix(RULE, options.clone(), source, Syntax::Css), source);
    assert_eq!(lint(RULE, options, source, Syntax::Css).len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(invalid_options(RULE, json!("number")).is_empty());
    assert_eq!(
      invalid_options(RULE, json!("degree")),
      ["Invalid option value \"degree\" for rule \"hue-degree-notation\""]
    );
  }
}
