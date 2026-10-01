use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::js_number::{parse_number, to_js_string, to_precision};
use crate::pattern::option_matches;
use crate::postcss_tree::PostcssTree;
use crate::rule::{Rule, RuleContext};
use crate::standard_syntax::is_standard_syntax_value;
use crate::value_parser::{self, NodeKind, ValueNode};

/// Specify percentage or number notation for alpha values.
///
/// Equivalent to Stylelint's `alpha-value-notation` rule, including its
/// autofix.  Checks the alpha channel of color functions (`rgb()`, `hsl()`,
/// `hwb()`, `lab()`, `lch()`, `oklab()`, `oklch()`, `color()`) and the values
/// of alpha properties such as `opacity`.  Primary option: `"number"` or
/// `"percentage"`; secondary option `exceptProperties` flips the expectation
/// for matching properties.
pub struct AlphaValueNotation;

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

impl Rule for AlphaValueNotation {
  fn name(&self) -> &'static str {
    "alpha-value-notation"
  }

  fn description(&self) -> &'static str {
    "Specify percentage or number notation for alpha values"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports every alpha value in the wrong notation, with a fix that
  /// rewrites just that value.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let primary = match ctx.primary_option_str() {
      Some("number") => Notation::Number,
      Some("percentage") => Notation::Percentage,
      _ => return Vec::new(),
    };
    let secondary = ctx.secondary_options();
    let tree = PostcssTree::parse(ctx.source, ctx.syntax);
    let mut diags = Vec::new();

    for decl in tree.decls() {
      let (Some(prop), Some(value)) = (
        ctx.source_slice(decl.name_span.start, decl.name_span.end),
        ctx.source_slice(decl.value_span.start, decl.value_span.end),
      ) else {
        continue;
      };
      let is_alpha_prop = ALPHA_PROPS.iter().any(|p| p.eq_ignore_ascii_case(prop));
      if !(is_alpha_prop || mentions_alpha_function(value)) || !has_digit(value) {
        continue;
      }
      let expected = if option_matches(secondary.and_then(|s| s.get("exceptProperties")), prop) {
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
          Diagnostic::new(
            self.name(),
            format!("Expected \"{unfixed}\" to be \"{fixed}\""),
          )
          .severity(self.default_severity())
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

  use crate::fix_testing::{fix, warnings};

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
      warnings(
        RULE,
        number(),
        "a { opacity: $a; color: rgb(0 0 0 / var(--a)) }",
        Syntax::Scss
      )
      .is_empty()
    );
    assert!(warnings(RULE, number(), "a { opacity: 0.5 }", Syntax::Css).is_empty());
  }

  #[test]
  fn reports_the_alpha_value_itself() {
    let source = "a { color: rgb(0 0 0 / 50%) }";
    let diags = warnings(RULE, json!(["number"]), source, Syntax::Css);
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
      warnings(RULE, options, "a { opacity: 10% }", Syntax::Css).len(),
      1
    );
  }
}
