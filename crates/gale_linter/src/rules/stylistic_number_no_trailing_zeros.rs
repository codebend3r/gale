use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/number-no-trailing-zeros";

declare_rule! {
  /// Disallow trailing zeros in numbers.
  ///
  /// Equivalent to `@stylistic/number-no-trailing-zeros`.
  pub struct StylisticNumberNoTrailingZeros => StylisticNumberNoTrailingZerosCheck {
    name: NAME,
    description: "Disallow trailing zeros in numbers",
    default_severity: Warning,
  }
}

/// `@stylistic/number-no-trailing-zeros`, which takes no options.
pub struct StylisticNumberNoTrailingZerosCheck;

impl StylisticNumberNoTrailingZerosCheck {
  /// The plugin validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for StylisticNumberNoTrailingZerosCheck {
  /// Checks the values of style-rule declarations, as written, for trailing
  /// zeros.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for decl in tree.walk_decls() {
      if !decl.parent_is_rule() {
        continue;
      }
      let value = decl.raw_value();
      let fixable = decl.value_fixable();
      find_trailing_zeros(&value, decl.value_start(), fixable, &mut diags);
    }
    diags
  }
}

/// Reports each number in `value`, which starts at `base_offset`, with a
/// redundant zero after the decimal point, at its first trailing zero as the
/// plugin does.  The fix, when `fixable`, rewrites the number.
fn find_trailing_zeros(
  value: &str,
  base_offset: usize,
  fixable: bool,
  diags: &mut Vec<Diagnostic>,
) {
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    // Skip url() contents — they contain encoded strings, not CSS numbers
    if i + 4 <= len && bytes[i..].starts_with(b"url(") {
      i += 4;
      let mut depth = 1;
      while i < len && depth > 0 {
        match bytes[i] {
          b'(' => depth += 1,
          b')' => depth -= 1,
          b'"' | b'\'' => {
            let q = bytes[i];
            i += 1;
            while i < len && bytes[i] != q {
              if bytes[i] == b'\\' && i + 1 < len {
                i += 1;
              }
              i += 1;
            }
          }
          _ => {}
        }
        if depth > 0 {
          i += 1;
        }
      }
      continue;
    }

    // Look for start of a number
    if bytes[i].is_ascii_digit()
      || (bytes[i] == b'.' && i + 1 < len && bytes[i + 1].is_ascii_digit())
    {
      let num_start = i;

      // Skip integer part
      while i < len && bytes[i].is_ascii_digit() {
        i += 1;
      }

      // Check for decimal point
      if i < len && bytes[i] == b'.' {
        i += 1; // skip the dot
        let frac_start = i;

        // Skip fractional digits
        while i < len && bytes[i].is_ascii_digit() {
          i += 1;
        }

        let frac_end = i;
        if frac_end > frac_start {
          let number = &value[num_start..frac_end];

          // Check for trailing zeros
          let trimmed = number.trim_end_matches('0');
          // Also remove trailing dot if all fractional digits were zeros
          let trimmed = trimmed.strip_suffix('.').unwrap_or(trimmed);

          if trimmed.len() < number.len() {
            // The first trailing zero, past the digits that stay.
            let zeros = frac_start + value[frac_start..frac_end].trim_end_matches('0').len();
            let mut diag =
              Diagnostic::new(NAME, format!("Unexpected trailing zero(s) in \"{number}\"")).span(
                Span::from_range(base_offset + zeros, base_offset + frac_end),
              );
            if fixable {
              diag = diag.fix(Fix::new(
                "Remove trailing zeros",
                vec![Edit::new(
                  Span::new(base_offset + num_start, number.len()),
                  trimmed,
                )],
              ));
            }
            diags.push(diag);
          }
        }
      }
      continue;
    }
    i += 1;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, invalid_options, lint};

  /// The reports for `a { opacity: <value>; }` in `syntax`.
  fn opacity(value: &str, syntax: Syntax) -> Vec<Diagnostic> {
    lint(
      NAME,
      json!(true),
      &format!("a {{ opacity: {value}; }}"),
      syntax,
    )
  }

  #[test]
  fn reports_trailing_zero_at_the_zero() {
    let d = opacity("1.0", Syntax::Css);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("1.0"));
    assert_eq!((d[0].span.offset, d[0].span.length), (15, 1));
    let fix = d[0].fix.as_ref().unwrap();
    assert_eq!(fix.edits[0].new_text, "1");
  }

  #[test]
  fn reports_multiple_trailing_zeros() {
    let d = opacity("0.500", Syntax::Scss);
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].span.offset, d[0].span.length), (16, 2));
    let fix = d[0].fix.as_ref().unwrap();
    assert_eq!(fix.edits[0].new_text, "0.5");
  }

  #[test]
  fn fixes_the_number_in_place() {
    let scss = "a { width: 1.50px; margin: 0 2.0em; }";
    assert_eq!(
      fix(NAME, json!(true), scss, Syntax::Scss),
      "a { width: 1.5px; margin: 0 2em; }"
    );
  }

  #[test]
  fn offers_no_fix_after_a_rewritten_line_comment() {
    let scss = "a { margin: 0 // x\n 1.50px; }";
    let d = lint(NAME, json!(true), scss, Syntax::Scss);
    assert_eq!(d.len(), 1);
    assert!(d[0].fix.is_none());
  }

  #[test]
  fn allows_no_trailing_zeros_and_integers() {
    assert!(opacity("0.5", Syntax::Css).is_empty());
    assert!(opacity("10", Syntax::Css).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"@stylistic/number-no-trailing-zeros\""]
    );
  }
}
