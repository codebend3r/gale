use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::js_number::parse_number;
use crate::pattern::option_matches;
use crate::postcss_tree::Node;
use crate::rule::{Rule, RuleContext};
use crate::standard_syntax::is_standard_syntax_value;
use crate::value_parser::{self, NodeKind, ValueNode};

/// Require numeric or named (where possible) `font-weight` values.
///
/// Equivalent to Stylelint's `font-weight-notation` rule, including its
/// autofix.  Checks `font-weight` (every weight, as in `@font-face` ranges)
/// and the weight in the `font` shorthand.
///
/// - `"numeric"`: `normal` and `bold` become `400` and `700`; `bolder` and
///   `lighter` are reported without a fix.
/// - `"named-where-possible"`: `400` and `700` become `normal` and `bold`.
///
/// Secondary option `ignore: ["relative"]` skips `bolder` and `lighter`.
pub struct FontWeightNotation;

/// Keywords that name a weight relative to the parent's.
const RELATIVE_KEYWORDS: &[&str] = &["bolder", "lighter"];

/// Keywords that are not numbers.
const NON_NUMERIC_KEYWORDS: &[&str] = &["bolder", "lighter", "normal", "bold"];

/// The notation the rule enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Notation {
  Numeric,
  NamedWherePossible,
}

impl Rule for FontWeightNotation {
  fn name(&self) -> &'static str {
    "font-weight-notation"
  }

  fn description(&self) -> &'static str {
    "Require numeric or named font-weight values"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports font weights in the other notation, with a fix wherever the
  /// weight has an equivalent.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let notation = match ctx.primary_option_str() {
      Some("numeric") => Notation::Numeric,
      Some("named-where-possible") => Notation::NamedWherePossible,
      _ => return Vec::new(),
    };
    let ignore_relative = option_matches(
      ctx.secondary_options().and_then(|s| s.get("ignore")),
      "relative",
    );
    let tree = ctx.postcss_tree();
    let mut diags = Vec::new();
    for decl in tree.decls() {
      self.check_decl(decl, ctx, notation, ignore_relative, &mut diags);
    }
    diags
  }
}

impl FontWeightNotation {
  /// Checks one `font` or `font-weight` declaration.
  fn check_decl(
    &self,
    decl: &Node,
    ctx: &RuleContext,
    notation: Notation,
    ignore_relative: bool,
    diags: &mut Vec<Diagnostic>,
  ) {
    let (Some(prop), Some(value)) = (
      ctx.source_slice(decl.name_span.start, decl.name_span.end),
      ctx.source_slice(decl.value_span.start, decl.value_span.end),
    ) else {
      return;
    };
    let is_shorthand = prop.eq_ignore_ascii_case("font");
    if !is_shorthand && !prop.eq_ignore_ascii_case("font-weight") {
      return;
    }
    let may_need_fix = match notation {
      Notation::Numeric => mentions_word(value, &["normal", "bold", "lighter", "bolder"], true),
      Notation::NamedWherePossible => mentions_word(value, &["400", "700"], false),
    };
    if !may_need_fix {
      return;
    }

    let nodes = value_parser::parse(value);
    let is_div = |i: Option<usize>| {
      i.and_then(|i| nodes.get(i))
        .is_some_and(|n| n.kind == NodeKind::Div)
    };
    let has_numeric_weight = nodes
      .iter()
      .enumerate()
      .any(|(i, n)| is_numbery(n.value) && !is_div(i.checked_sub(1)));

    for (i, node) in nodes.iter().enumerate() {
      // A possible weight: a word not next to a divider (so not the
      // `16px/3` of a shorthand).
      if node.kind != NodeKind::Word || is_div(i.checked_sub(1)) || is_div(Some(i + 1)) {
        continue;
      }
      if is_shorthand {
        if node.value.eq_ignore_ascii_case("normal") && has_numeric_weight {
          // This `normal` is another longhand of the shorthand.
          continue;
        }
        if let Some(diag) =
          self.check_weight(node, decl.value_span.start, notation, ignore_relative)
        {
          diags.push(diag);
          // The shorthand has one weight.
          break;
        }
      } else if let Some(diag) =
        self.check_weight(node, decl.value_span.start, notation, ignore_relative)
      {
        diags.push(diag);
      }
    }
  }

  /// The problem with the weight `node` (offsets relative to `base`), if it
  /// is in the wrong notation.
  fn check_weight(
    &self,
    node: &ValueNode<'_>,
    base: usize,
    notation: Notation,
    ignore_relative: bool,
  ) -> Option<Diagnostic> {
    let weight = node.value;
    if !is_standard_syntax_value(weight) {
      return None;
    }
    let lower = weight.to_ascii_lowercase();
    if lower.starts_with("var(") || (ignore_relative && RELATIVE_KEYWORDS.contains(&lower.as_str()))
    {
      return None;
    }
    let span = Span::new(base + node.source_index, weight.len());
    let (message, replacement) = match notation {
      Notation::Numeric => {
        if is_numbery(&lower) || !NON_NUMERIC_KEYWORDS.contains(&lower.as_str()) {
          return None;
        }
        match lower.as_str() {
          "normal" => (format!("Expected \"{weight}\" to be \"400\""), Some("400")),
          "bold" => (format!("Expected \"{weight}\" to be \"700\""), Some("700")),
          _ => ("Expected numeric font-weight notation".to_string(), None),
        }
      }
      Notation::NamedWherePossible => {
        let named = match lower.as_str() {
          "400" => "normal",
          "700" => "bold",
          _ => return None,
        };
        (
          format!("Expected \"{weight}\" to be \"{named}\""),
          Some(named),
        )
      }
    };
    let diag = Diagnostic::new(self.name(), message)
      .severity(self.default_severity())
      .span(span);
    Some(match replacement {
      Some(text) => diag.fix(Fix::new(
        format!("Replace \"{weight}\" with \"{text}\""),
        vec![Edit::new(span, text)],
      )),
      None => diag,
    })
  }
}

/// Stylelint's `isNumbery`: not blank, and `Number(value) == value`.
fn is_numbery(value: &str) -> bool {
  !value.trim().is_empty() && !parse_number(value).is_nan()
}

/// Whether one of `words` appears in `value` between word boundaries
/// (Stylelint's `/\b(?:...)\b/`, case-insensitive when `ignore_case`).
fn mentions_word(value: &str, words: &[&str], ignore_case: bool) -> bool {
  let haystack = if ignore_case {
    value.to_ascii_lowercase()
  } else {
    value.to_string()
  };
  let bytes = haystack.as_bytes();
  let is_word = |i: usize| {
    bytes
      .get(i)
      .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
  };
  words.iter().any(|word| {
    haystack.match_indices(word).any(|(start, _)| {
      let end = start + word.len();
      (start == 0 || !is_word(start - 1)) && !is_word(end)
    })
  })
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, lint};

  const RULE: &str = "font-weight-notation";

  #[test]
  fn fixes_named_weights_to_numbers() {
    let numeric = || json!(["numeric"]);
    assert_eq!(
      fix(RULE, numeric(), "a { FONT-WEIGHT: NORMAL; }", Syntax::Css),
      "a { FONT-WEIGHT: 400; }"
    );
    assert_eq!(
      fix(
        RULE,
        numeric(),
        "a { font-weight: /* bold */ normal; }",
        Syntax::Css
      ),
      "a { font-weight: /* bold */ 400; }"
    );
    assert_eq!(
      fix(
        RULE,
        numeric(),
        "@font-face { font-weight: normal bold; }",
        Syntax::Css
      ),
      "@font-face { font-weight: 400 700; }"
    );
    assert_eq!(
      fix(
        RULE,
        numeric(),
        "a { font: normal normal 16px/3 cursive; }",
        Syntax::Css
      ),
      "a { font: 400 normal 16px/3 cursive; }"
    );
  }

  #[test]
  fn fixes_numbers_to_names_where_possible() {
    let named = || json!(["named-where-possible"]);
    assert_eq!(
      fix(
        RULE,
        named(),
        "a { font: italic small-caps 700 16px/3 cursive; }",
        Syntax::Css
      ),
      "a { font: italic small-caps bold 16px/3 cursive; }"
    );
    assert_eq!(
      fix(
        RULE,
        named(),
        "a { font: 400 normal 16px serif; }",
        Syntax::Css
      ),
      "a { font: normal normal 16px serif; }"
    );
  }

  #[test]
  fn relative_keywords_are_reported_without_a_fix() {
    let source = "a { font: italic small-caps bolder 16px/3 cursive; }";
    let diags = lint(RULE, json!(["numeric"]), source, Syntax::Css);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].message, "Expected numeric font-weight notation");
    assert_eq!(fix(RULE, json!(["numeric"]), source, Syntax::Css), source);
    let ignored = json!(["numeric", { "ignore": ["relative"] }]);
    assert!(lint(RULE, ignored, source, Syntax::Css).is_empty());
  }

  #[test]
  fn leaves_line_heights_and_variables_alone() {
    let numeric = || json!(["numeric"]);
    assert!(lint(RULE, numeric(), "a { font-weight: $bold; }", Syntax::Scss).is_empty());
    assert!(
      lint(
        RULE,
        json!(["named-where-possible"]),
        "a { font: 16px/400 serif; }",
        Syntax::Css
      )
      .is_empty()
    );
  }

  #[test]
  fn multibyte_values_do_not_panic() {
    let numeric = || json!(["numeric"]);
    assert_eq!(
      fix(RULE, numeric(), "a { font: bold 1em \"é\"; }", Syntax::Css),
      "a { font: 700 1em \"é\"; }"
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = json!(["numeric", { "disableFix": true }]);
    let source = "a { font-weight: bold; }";
    assert_eq!(fix(RULE, options.clone(), source, Syntax::Css), source);
    assert_eq!(lint(RULE, options, source, Syntax::Css).len(), 1);
  }
}
