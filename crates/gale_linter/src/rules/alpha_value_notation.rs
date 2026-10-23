use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::value_parser::{self, NodeKind, ValueNode};
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::js_number::{parse_number, to_js_string, to_precision};
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::standard_syntax::is_standard_syntax_value;

const NAME: &str = "alpha-value-notation";

declare_rule! {
  /// Specify percentage or number notation for alpha values.
  ///
  /// Equivalent to Stylelint's `alpha-value-notation` rule, including its
  /// autofix.  Checks the alpha channel of color functions (`rgb()`, `hsl()`,
  /// `hwb()`, `lab()`, `lch()`, `oklab()`, `oklch()`, `color()`) and the values
  /// of alpha properties such as `opacity`.  Primary option: `"number"` or
  /// `"percentage"`; secondary option `exceptProperties` flips the expectation
  /// for matching properties.
  pub struct AlphaValueNotation => AlphaValueNotationCheck {
    name: NAME,
    description: "Specify percentage or number notation for alpha values",
    default_severity: Warning,
  }
}

/// `alpha-value-notation` as configured.
pub struct AlphaValueNotationCheck {
  /// The notation alpha values should use; `None` when the config names
  /// none, and the rule checks nothing.
  notation: Option<Notation>,
  /// `exceptProperties`.
  except_properties: Matchers,
}

impl AlphaValueNotationCheck {
  /// Stylelint's options: `"number"` or `"percentage"`, then
  /// `exceptProperties`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let notation = options.primary_one_of(&["number", "percentage"]);
    let except_properties = options.matchers("exceptProperties");
    options.finish()?;
    Ok(Self {
      notation: notation.map(|notation| match notation {
        "number" => Notation::Number,
        _ => Notation::Percentage,
      }),
      except_properties,
    })
  }
}

/// Properties whose value is an alpha value.
const ALPHA_PROPS: &[&str] = &[
  "opacity",
  "shape-image-threshold",
  "fill-opacity",
  "flood-opacity",
  "stop-opacity",
  "stroke-opacity",
];

/// Color functions that take an alpha channel.
const ALPHA_FUNCTIONS: &[&str] = &[
  "color", "hsl", "hsla", "rgb", "rgba", "hwb", "lab", "lch", "oklab", "oklch",
];

/// The notation alpha values should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Notation {
  Number,
  Percentage,
}

impl Notation {
  /// The other notation, for properties listed in `exceptProperties`.
  fn flipped(self) -> Self {
    match self {
      Notation::Number => Notation::Percentage,
      Notation::Percentage => Notation::Number,
    }
  }
}

impl Check for AlphaValueNotationCheck {
  /// Reports every alpha value in the wrong notation, with a fix that
  /// rewrites just that value.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let Some(primary) = self.notation else {
      return Vec::new();
    };
    let mut diags = Vec::new();

    for decl in tree.walk_decls() {
      let Some(prop) = ctx.source_slice(decl.name_span.start, decl.name_span.end) else {
        continue;
      };
      let value = decl.written_value();
      let is_alpha_prop = ALPHA_PROPS.iter().any(|p| p.eq_ignore_ascii_case(prop));
      if !(is_alpha_prop || mentions_alpha_function(value)) || !has_digit(value) {
        continue;
      }
      let expected = if self.except_properties.matches(prop) {
        primary.flipped()
      } else {
        primary
      };

      let parsed = value_parser::parse(value);
      let mut check = |node: &ValueNode<'_>| {
        let alpha = if is_alpha_prop && has_digit(node.value) {
          (node.kind == NodeKind::Word || node.kind == NodeKind::Function).then_some(node)
        } else if node.kind == NodeKind::Function && is_alpha_function(node.value) {
          find_alpha_in_function(node)
        } else {
          None
        };
        let Some(alpha) = alpha else { return };
        let unfixed = alpha.value;
        if !is_standard_syntax_value(unfixed) {
          return;
        }
        let fixed = match (expected, value_parser::unit(unfixed)) {
          (Notation::Number, Some((number, "%"))) => as_number(number),
          (Notation::Percentage, Some((_, ""))) => as_percentage(unfixed),
          _ => return,
        };
        let start = decl.value_span.start + alpha.source_index;
        let span = Span::new(start, unfixed.len());
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

/// Whether `value` contains a call to one of the alpha color functions
/// (Stylelint tests `/(?:color|hsla?|rgba?|hwb|lab|lch|oklab|oklch)\(/i`).
fn mentions_alpha_function(value: &str) -> bool {
  let lower = value.to_ascii_lowercase();
  lower.match_indices('(').any(|(paren, _)| {
    ALPHA_FUNCTIONS
      .iter()
      .any(|name| lower[..paren].ends_with(name))
  })
}

/// Whether `name` is one of the alpha color functions, in any case.
fn is_alpha_function(name: &str) -> bool {
  ALPHA_FUNCTIONS.iter().any(|f| f.eq_ignore_ascii_case(name))
}

/// Whether `text` contains an ASCII digit.
fn has_digit(text: &str) -> bool {
  text.bytes().any(|b| b.is_ascii_digit())
}

/// The alpha argument of a color function: the fourth argument of the comma
/// syntax, or the first word after the `/` of the space syntax.
fn find_alpha_in_function<'n, 'a>(node: &'n ValueNode<'a>) -> Option<&'n ValueNode<'a>> {
  if node.nodes.iter().any(|n| n.is_comma()) {
    let args: Vec<&ValueNode<'a>> = node
      .nodes
      .iter()
      .filter(|n| n.kind == NodeKind::Word || n.kind == NodeKind::Function)
      .collect();
    return if args.len() == 4 { Some(args[3]) } else { None };
  }
  let slash = node.nodes.iter().position(|n| n.is_slash())?;
  node.nodes[slash + 1..]
    .iter()
    .find(|n| n.kind == NodeKind::Word)
}

/// `${Number((Number(value) * 100).toPrecision(3))}%`
fn as_percentage(value: &str) -> String {
  let scaled = parse_number(value) * 100.0;
  format!("{}%", to_js_string(parse_number(&to_precision(scaled, 3))))
}

/// `Number((Number(number) / 100).toPrecision(3)).toString()`
fn as_number(number: &str) -> String {
  let scaled = parse_number(number) / 100.0;
  to_js_string(parse_number(&to_precision(scaled, 3)))
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, invalid_options, lint};

  const RULE: &str = "alpha-value-notation";

  #[test]
  fn fixes_to_numbers() {
    let number = || json!(["number"]);
    assert_eq!(
      fix(RULE, number(), "a { opacity: 10% }", Syntax::Css),
      "a { opacity: 0.1 }"
    );
    assert_eq!(
      fix(RULE, number(), "a { opacity: 0.3% }", Syntax::Css),
      "a { opacity: 0.003 }"
    );
    assert_eq!(
      fix(
        RULE,
        number(),
        "a { color: HSL(198DEG 28% 50% / 10%) }",
        Syntax::Css
      ),
      "a { color: HSL(198DEG 28% 50% / 0.1) }"
    );
    assert_eq!(
      fix(
        RULE,
        number(),
        "a { color: rgba(0, 0, 0, 50%/*comment*/) }",
        Syntax::Css
      ),
      "a { color: rgba(0, 0, 0, 0.5/*comment*/) }"
    );
    assert_eq!(
      fix(
        RULE,
        number(),
        "a { color: rgb(from rgb(127 127 127 / 25%) 0 0 0 / 50%) }",
        Syntax::Css
      ),
      "a { color: rgb(from rgb(127 127 127 / 0.25) 0 0 0 / 0.5) }"
    );
    assert_eq!(
      fix(RULE, number(), "$a: rgb(0 0 0 / 50%);", Syntax::Scss),
      "$a: rgb(0 0 0 / 0.5);"
    );
  }

  #[test]
  fn fixes_to_percentages() {
    let percentage = || json!(["percentage"]);
    assert_eq!(
      fix(RULE, percentage(), "a { opacity: 0.14 }", Syntax::Css),
      "a { opacity: 14% }"
    );
    assert_eq!(
      fix(
        RULE,
        percentage(),
        "a { color: rgba(0, 0, 0, .5) }",
        Syntax::Css
      ),
      "a { color: rgba(0, 0, 0, 50%) }"
    );
    assert_eq!(
      fix(
        RULE,
        percentage(),
        "@keyframes k { 0% { opacity: 0.5 } }",
        Syntax::Css
      ),
      "@keyframes k { 0% { opacity: 50% } }"
    );
  }

  #[test]
  fn except_properties_flip_the_notation() {
    let options = json!(["number", { "exceptProperties": ["opacity"] }]);
    assert_eq!(
      fix(
        RULE,
        options,
        "a { opacity: 0.1; color: rgb(0 0 0 / 70%) }",
        Syntax::Css
      ),
      "a { opacity: 10%; color: rgb(0 0 0 / 0.7) }"
    );
  }

  #[test]
  fn leaves_variables_and_correct_values_alone() {
    let number = || json!(["number"]);
    assert!(
      lint(
        RULE,
        number(),
        "a { opacity: $a; color: rgb(0 0 0 / var(--a)) }",
        Syntax::Scss
      )
      .is_empty()
    );
    assert!(lint(RULE, number(), "a { opacity: 0.5 }", Syntax::Css).is_empty());
  }

  #[test]
  fn reports_the_alpha_value_itself() {
    let source = "a { color: rgb(0 0 0 / 50%) }";
    let diags = lint(RULE, json!(["number"]), source, Syntax::Css);
    assert_eq!(diags.len(), 1);
    assert_eq!(&source[diags[0].span.offset..diags[0].span.end()], "50%");
    assert_eq!(diags[0].message, "Expected \"50%\" to be \"0.5\"");
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = json!(["number", { "disableFix": true }]);
    assert_eq!(
      fix(RULE, options.clone(), "a { opacity: 10% }", Syntax::Css),
      "a { opacity: 10% }"
    );
    assert_eq!(
      lint(RULE, options, "a { opacity: 10% }", Syntax::Css).len(),
      1
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        RULE,
        json!(["percentage", { "exceptProperties": ["opacity", "/^fill/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(RULE, json!("decimal")),
      ["Invalid option value \"decimal\" for rule \"alpha-value-notation\""]
    );
    assert_eq!(
      invalid_options(RULE, json!(["number", { "exceptProperties": [1] }])),
      ["Invalid value \"1\" for option \"exceptProperties\" of rule \"alpha-value-notation\""]
    );
  }
}
