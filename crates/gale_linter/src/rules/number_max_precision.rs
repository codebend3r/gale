use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{Accept, InvalidOption, Setting, is_object_of_non_negative_integers};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "number-max-precision";

declare_rule! {
  /// Reports numbers with more than 4 decimal places.
  ///
  /// Equivalent to Stylelint's `number-max-precision` rule (default: 4).
  pub struct NumberMaxPrecision => NumberMaxPrecisionCheck {
    name: NAME,
    description: "Limit the number of decimal places allowed in numbers",
    default_severity: Warning,
  }
}

/// `number-max-precision` as configured.
pub struct NumberMaxPrecisionCheck {
  /// The most decimal places a number may have.
  max: usize,
}

/// The precision allowed when the config gives none.
const MAX_PRECISION: usize = 4;

impl NumberMaxPrecisionCheck {
  /// Stylelint's options: a whole number of zero or more, then
  /// `ignoreProperties`, `ignoreUnits` and `insideFunctions`.  Gale reads no
  /// number as 4, and accepts but does not yet apply the secondary options.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max = options.primary_number(&[Accept::NonNegativeInteger]);
    options.strings("ignoreProperties");
    options.strings("ignoreUnits");
    options.value(
      "insideFunctions",
      &[Accept::Test(is_object_of_non_negative_integers)],
    );
    options.finish()?;
    Ok(Self {
      max: max.map_or(MAX_PRECISION, |max| max as usize),
    })
  }
}

impl Check for NumberMaxPrecisionCheck {
  /// Flags the first number in each style-rule declaration with more decimal
  /// places than allowed, reading the value as written.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for decl in tree.walk_decls() {
      if !decl.parent_is_rule() {
        continue;
      }
      let value = decl.raw_value();
      let Some(found) = find_precision_issue(&value, self.max) else {
        continue;
      };
      let (original, rounded) = (found.original, found.rounded);
      diags.push(
        Diagnostic::new(NAME, format!("Expected \"{original}\" to be \"{rounded}\""))
          .span(Span::new(decl.value_start() + found.offset, original.len())),
      );
    }
    diags
  }
}

/// A number in a value that carries more decimal places than allowed.
struct PrecisionIssue {
  /// The number exactly as written in the scanned text.
  original: String,
  /// The same number rounded to the configured precision.
  rounded: String,
  /// Byte offset of the number within the scanned text.
  offset: usize,
}

/// Find the first number in `value` that exceeds `max` decimal places.
///
/// Scans bytes rather than chars so the returned offset can be used directly as
/// a source position. This is safe for non-ASCII input because no byte of a
/// multi-byte UTF-8 sequence can equal an ASCII `.` or digit.
fn find_precision_issue(value: &str, max: usize) -> Option<PrecisionIssue> {
  let bytes = value.as_bytes();
  let len = bytes.len();

  for i in 0..len {
    if bytes[i] != b'.' {
      continue;
    }

    // Walk back over the integer part.
    let mut num_start = i;
    while num_start > 0 && bytes[num_start - 1].is_ascii_digit() {
      num_start -= 1;
    }

    // Walk forward over the fractional part.
    let mut j = i + 1;
    let mut decimal_digits = 0;
    while j < len && bytes[j].is_ascii_digit() {
      decimal_digits += 1;
      j += 1;
    }

    if decimal_digits > max {
      let original = value[num_start..j].to_string();
      let rounded = round_to_precision(&original, max);
      return Some(PrecisionIssue {
        original,
        rounded,
        offset: num_start,
      });
    }
  }
  None
}

/// Round a decimal number string to `max` decimal places.
fn round_to_precision(num: &str, max: usize) -> String {
  if let Ok(f) = num.parse::<f64>() {
    let factor = 10f64.powi(max as i32);
    let rounded = (f * factor).round() / factor;
    if max == 0 {
      format!("{}", rounded as i64)
    } else {
      format!("{:.prec$}", rounded, prec = max)
    }
  } else {
    num.to_string()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use serde_json::{Value, json};

  use crate::testing::{invalid_options, lint};

  /// The reports for `a { width: <value>; }` in CSS.
  fn width(options: Value, value: &str) -> Vec<Diagnostic> {
    lint(
      NAME,
      options,
      &format!("a {{ width: {value}; }}"),
      Syntax::Css,
    )
  }

  #[test]
  fn reports_excess_precision() {
    assert_eq!(width(json!(4), "0.12345em").len(), 1);
  }

  #[test]
  fn allows_within_precision() {
    assert!(width(json!(4), "0.1234em").is_empty());
    assert!(width(json!(4), "10px").is_empty());
  }

  #[test]
  fn reports_excess_in_multiple_values() {
    assert_eq!(width(json!(4), "0.12345 0.6789").len(), 1);
  }

  #[test]
  fn checks_only_style_rule_declarations() {
    let css = "@font-face { size-adjust: 1.123456%; }\n\
      a { @media print { top: 1.123456px; } b { top: 1.123456px; } }";
    let d = lint(NAME, json!(4), css, Syntax::Css);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn honours_array_form_primary_option() {
    // `[2, { ... }]` is Stylelint's standard shape; the max must be read
    // from element 0, not from the array itself.
    let d = width(json!([2, {}]), "1.234px");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("to be \"1.23\""), "{}", d[0].message);
  }

  #[test]
  fn array_form_can_disable_by_raising_max() {
    assert!(width(json!([6, {}]), "1.234px").is_empty());
  }

  #[test]
  fn reports_the_source_literal_at_its_column() {
    let source = "a { width: 1.123456789px !important; }";
    let d = lint(NAME, json!(2), source, Syntax::Css);
    assert_eq!(d.len(), 1);
    assert!(
      d[0].message.contains("\"1.123456789\""),
      "should quote the source literal, got: {}",
      d[0].message
    );
    assert_eq!(
      (d[0].span.offset, d[0].span.length),
      (source.find("1.123456789").unwrap(), 11),
      "should point at the number, not the declaration start"
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([2, {
          "ignoreProperties": ["/^my-/"],
          "ignoreUnits": ["px"],
          "insideFunctions": { "/^(?:rgb|hsl)a?$/": 0 }
        }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(-1)),
      ["Invalid option value \"-1\" for rule \"number-max-precision\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(1.5)),
      ["Invalid option value \"1.5\" for rule \"number-max-precision\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([2, { "insideFunctions": { "rgb": "x" } }])
      ),
      [
        "Invalid value \"{\"rgb\":\"x\"}\" for option \"insideFunctions\" of rule \"number-max-precision\""
      ]
    );
  }
}
