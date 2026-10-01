use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::js_number::{parse_float, to_js_string, to_precision};
use crate::postcss_tree::PostcssTree;
use crate::rule::{Rule, RuleContext};
use crate::standard_syntax::is_standard_syntax_value;
use crate::value_parser::{self, NodeKind, ValueNode};

/// Specify number or percentage notation for lightness.
///
/// Equivalent to Stylelint's `lightness-notation` rule, including its
/// autofix.  Checks the lightness channel of `lab()`, `lch()`, `oklab()` and
/// `oklch()`, also in relative color syntax.  Primary option: `"percentage"`
/// or `"number"`.  `oklab()`/`oklch()` lightness runs from 0 to 1 as a
/// number, so their fixes scale by 100; `lab()`/`lch()` keep the number.
pub struct LightnessNotation;

/// Functions whose numeric lightness runs from 0 to 1.
const ZERO_TO_ONE: &[&str] = &["oklab", "oklch"];

/// Functions whose numeric lightness runs from 0 to 100.
const ZERO_TO_HUNDRED: &[&str] = &["lab", "lch"];

impl Rule for LightnessNotation {
  fn name(&self) -> &'static str {
    "lightness-notation"
  }

  fn description(&self) -> &'static str {
    "Specify number or percentage notation for lightness values"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports every lightness in the wrong notation, with a fix that converts
  /// it the way Stylelint does.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let want_percentage = match ctx.primary_option_str() {
      Some("percentage") => true,
      Some("number") => false,
      _ => return Vec::new(),
    };
    let tree = PostcssTree::parse(ctx.source, ctx.syntax);
    let mut diags = Vec::new();

    for decl in tree.decls() {
      let Some(value) = ctx.source_slice(decl.value_span.start, decl.value_span.end) else {
        continue;
      };
      if !mentions_lightness_function(value) {
        continue;
      }
      let parsed = value_parser::parse(value);
      let mut check = |node: &ValueNode<'_>| {
        if node.kind != NodeKind::Function {
          return;
        }
        let function = node.value.to_ascii_lowercase();
        if !ZERO_TO_ONE.contains(&function.as_str())
          && !ZERO_TO_HUNDRED.contains(&function.as_str())
        {
          return;
        }
        let Some(lightness) = find_lightness(node) else {
          return;
        };
        let unfixed = lightness.value;
        if !is_standard_syntax_value(unfixed) {
          return;
        }
        let Some((_, unit)) = value_parser::unit(unfixed) else {
          return;
        };
        let is_percentage = unit == "%";
        let is_number = unit.is_empty();
        if !(is_percentage || is_number)
          || (want_percentage && is_percentage)
          || (!want_percentage && is_number)
        {
          return;
        }
        let fixed = if want_percentage {
          as_percentage(unfixed, &function)
        } else {
          as_number(unfixed, &function)
        };
        let start = decl.value_span.start + lightness.source_index;
        let span = Span::new(start, lightness.source_end_index - lightness.source_index);
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

/// Whether `value` calls a lightness color function (Stylelint tests
/// `/\b(?:oklab|oklch|lab|lch)\(/i`).
fn mentions_lightness_function(value: &str) -> bool {
  let lower = value.to_ascii_lowercase();
  lower.match_indices('(').any(|(paren, _)| {
    ZERO_TO_ONE.iter().chain(ZERO_TO_HUNDRED).any(|name| {
      lower[..paren].ends_with(name) && {
        let start = paren - name.len();
        start == 0 || {
          let prev = lower.as_bytes()[start - 1];
          !(prev.is_ascii_alphanumeric() || prev == b'_')
        }
      }
    })
  })
}

/// The lightness argument: the first channel, after `from <color>` in
/// relative color syntax.
fn find_lightness<'n, 'a>(node: &'n ValueNode<'a>) -> Option<&'n ValueNode<'a>> {
  let args: Vec<&ValueNode<'a>> = node
    .nodes
    .iter()
    .filter(|n| n.kind == NodeKind::Word || n.kind == NodeKind::Function)
    .collect();
  let relative = args
    .first()
    .is_some_and(|a| a.value.eq_ignore_ascii_case("from"));
  args.get(if relative { 2 } else { 0 }).copied()
}

/// Stylelint's `asPercentage`.
fn as_percentage(value: &str, function: &str) -> String {
  let mut num = parse_float(value);
  if ZERO_TO_HUNDRED.contains(&function) {
    return format!("{}%", to_js_string(num));
  }
  if ZERO_TO_ONE.contains(&function) {
    num *= 100.0;
  }
  if num.is_finite() && num.fract() == 0.0 {
    return format!("{}%", to_js_string(num));
  }
  format!("{}%", round_to_number_of_digits(num, value))
}

/// Stylelint's `asNumber`.
fn as_number(value: &str, function: &str) -> String {
  let num = parse_float(value);
  if ZERO_TO_ONE.contains(&function) {
    return round_to_number_of_digits(num / 100.0, value);
  }
  to_js_string(num)
}

/// Stylelint's `roundToNumberOfDigits`: `num` to as many significant digits
/// as `value` has, with trailing zeros (and a bare point) trimmed.
fn round_to_number_of_digits(num: f64, value: &str) -> String {
  if num == 0.0 {
    return "0".to_string();
  }
  let precision = value.replace(['%', '.'], "").chars().count();
  if precision == 0 {
    return to_js_string(num);
  }
  let rounded = to_precision(num, precision);
  // `.replace(/\.?0+$/, '')`
  let without_zeros = rounded.trim_end_matches('0');
  if without_zeros.len() == rounded.len() {
    return rounded;
  }
  without_zeros
    .strip_suffix('.')
    .unwrap_or(without_zeros)
    .to_string()
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use super::round_to_number_of_digits;
  use crate::fix_testing::{fix, warnings};

  const RULE: &str = "lightness-notation";

  #[test]
  fn fixes_to_percentages() {
    let percentage = || json!(["percentage"]);
    assert_eq!(
      fix(
        RULE,
        percentage(),
        "a { color: oklch(0.5 0.2 120) }",
        Syntax::Css
      ),
      "a { color: oklch(50% 0.2 120) }"
    );
    assert_eq!(
      fix(
        RULE,
        percentage(),
        "a { color: oklab(0.123 0.1 0.1) }",
        Syntax::Css
      ),
      "a { color: oklab(12.3% 0.1 0.1) }"
    );
    assert_eq!(
      fix(
        RULE,
        percentage(),
        "a { color: LCH(56.29 19.86 10) }",
        Syntax::Css
      ),
      "a { color: LCH(56.29% 19.86 10) }"
    );
    assert_eq!(
      fix(
        RULE,
        percentage(),
        "a { color: lab(from red 50 a b) }",
        Syntax::Css
      ),
      "a { color: lab(from red 50% a b) }"
    );
  }

  #[test]
  fn fixes_to_numbers() {
    let number = || json!(["number"]);
    assert_eq!(
      fix(
        RULE,
        number(),
        "a { color: oklch(56.29% 0.2 120) }",
        Syntax::Css
      ),
      "a { color: oklch(0.5629 0.2 120) }"
    );
    assert_eq!(
      fix(
        RULE,
        number(),
        "a { color: lch(50% 19.86 10) }",
        Syntax::Css
      ),
      "a { color: lch(50 19.86 10) }"
    );
  }

  #[test]
  fn rounds_like_stylelint() {
    assert_eq!(round_to_number_of_digits(0.5, "50%"), "0.5");
    assert_eq!(round_to_number_of_digits(1.0, "100%"), "1");
    assert_eq!(round_to_number_of_digits(0.0, "0%"), "0");
    assert_eq!(round_to_number_of_digits(12.3456, "0.123456"), "12.3456");
  }

  #[test]
  fn ignores_variables_and_other_functions() {
    let percentage = || json!(["percentage"]);
    assert!(
      warnings(
        RULE,
        percentage(),
        "a { color: oklch($l 0.2 120) }",
        Syntax::Scss
      )
      .is_empty()
    );
    assert!(
      warnings(
        RULE,
        percentage(),
        "a { color: hsl(120 60 70) }",
        Syntax::Css
      )
      .is_empty()
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = json!(["percentage", { "disableFix": true }]);
    let source = "a { color: oklch(0.5 0.2 120) }";
    assert_eq!(fix(RULE, options.clone(), source, Syntax::Css), source);
    assert_eq!(warnings(RULE, options, source, Syntax::Css).len(), 1);
  }
}
