use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/unit-case";

declare_rule! {
  /// Specify lowercase or uppercase for units.
  ///
  /// Equivalent to `@stylistic/unit-case`.
  pub struct StylisticUnitCase => StylisticUnitCaseCheck {
    name: NAME,
    description: "Specify lowercase or uppercase for units",
    default_severity: Warning,
  }
}

/// `@stylistic/unit-case` as configured.
pub struct StylisticUnitCaseCheck {
  /// The primary option: `"lower"` or `"upper"`.
  expected_case: &'static str,
}

impl StylisticUnitCaseCheck {
  /// The plugin's options: `"lower"` or `"upper"`.  Gale reads no option as
  /// `"lower"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expected_case = options.primary_one_of(&["lower", "upper"]);
    options.finish()?;
    Ok(Self {
      expected_case: expected_case.unwrap_or("lower"),
    })
  }
}

impl Check for StylisticUnitCaseCheck {
  /// Flags units in declaration values, as written, whose case does not
  /// match the option, at the unit, with a fix that recases it.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let expected_case = self.expected_case;
    let mut diags = Vec::new();
    for decl in tree.walk_decls() {
      let value = decl.raw_value();
      let fixable = decl.value_fixable();
      let base = decl.value_start();
      for (unit_offset, unit) in find_units(&value) {
        let fixed = match expected_case {
          "lower" => unit.to_ascii_lowercase(),
          "upper" => unit.to_ascii_uppercase(),
          _ => continue,
        };
        if fixed == unit {
          continue;
        }
        let span = Span::new(base + unit_offset, unit.len());
        let mut diag =
          Diagnostic::new(NAME, format!("Expected \"{unit}\" to be \"{fixed}\"")).span(span);
        if fixable {
          diag = diag.fix(Fix::new(
            format!("Convert to {expected_case}case"),
            vec![Edit::new(span, &fixed)],
          ));
        }
        diags.push(diag);
      }
    }
    diags
  }
}

/// Find units in a CSS value string. Returns (offset, unit_str) pairs.
fn find_units(value: &str) -> Vec<(usize, String)> {
  let mut units = Vec::new();
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    // Skip quoted strings (e.g. content: "\00A0")
    if bytes[i] == b'"' || bytes[i] == b'\'' {
      let quote = bytes[i];
      i += 1;
      while i < len && bytes[i] != quote {
        if bytes[i] == b'\\' {
          i += 1;
        }
        i += 1;
      }
      if i < len {
        i += 1;
      }
      continue;
    }

    // Skip hex colours (`#97C1FF`) and unicode ranges (`U+1D00-1DBF`),
    // whose digits are not numbers.
    let hex = bytes[i] == b'#';
    let unicode_range = matches!(bytes[i], b'u' | b'U') && bytes.get(i + 1) == Some(&b'+');
    if hex || unicode_range {
      i += 1 + usize::from(unicode_range);
      while i < len && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'-' | b'?')) {
        i += 1;
      }
      continue;
    }

    // Skip function calls (including var()), since their arguments may
    // contain digits followed by letters that are not CSS units
    // (e.g. var(--base0C) where `C` is part of a custom property name).
    if bytes[i] == b'(' {
      // Skip the entire parenthesised argument list (nested parens allowed)
      let mut depth = 1;
      i += 1;
      while i < len && depth > 0 {
        if bytes[i] == b'(' {
          depth += 1;
        } else if bytes[i] == b')' {
          depth -= 1;
        } else if bytes[i] == b'"' || bytes[i] == b'\'' {
          let q = bytes[i];
          i += 1;
          while i < len && bytes[i] != q {
            if bytes[i] == b'\\' {
              i += 1;
            }
            i += 1;
          }
        }
        i += 1;
      }
      continue;
    }

    // Skip past a number (digits, dots)
    if bytes[i].is_ascii_digit()
      || (bytes[i] == b'.' && i + 1 < len && bytes[i + 1].is_ascii_digit())
    {
      // Check that the digit is not part of an identifier (preceded by
      // alphanumeric or `-` which would make it a name, not a number).
      let preceded_by_ident = i > 0
        && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'-' || bytes[i - 1] == b'_');
      if preceded_by_ident {
        i += 1;
        continue;
      }
      while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
        i += 1;
      }
      // Now check for a unit (alphabetic chars or %)
      if i < len && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'%') {
        let unit_start = i;
        while i < len && bytes[i].is_ascii_alphabetic() {
          i += 1;
        }
        if i > unit_start {
          let unit = &value[unit_start..i];
          units.push((unit_start, unit.to_string()));
        }
      }
    } else {
      i += 1;
    }
  }
  units
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, invalid_options, lint};

  /// The reports for `a { width: <value>; }` in `syntax`, lower case wanted.
  fn width(value: &str, syntax: Syntax) -> Vec<Diagnostic> {
    lint(
      NAME,
      json!("lower"),
      &format!("a {{ width: {value}; }}"),
      syntax,
    )
  }

  #[test]
  fn reports_uppercase_unit_at_the_unit() {
    for syntax in [Syntax::Css, Syntax::Scss] {
      let d = width("10PX", syntax);
      assert_eq!(d.len(), 1);
      assert!(d[0].message.contains("\"px\""));
      assert_eq!((d[0].span.offset, d[0].span.length), (13, 2));
    }
  }

  #[test]
  fn allows_lowercase_unit() {
    assert!(width("10px", Syntax::Css).is_empty());
  }

  #[test]
  fn reports_mixed_case_unit() {
    let d = width("10Rem", Syntax::Css);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("\"rem\""));
  }

  #[test]
  fn fixes_the_unit_in_place() {
    let scss = "a { width: 10PX; @media print { top: 1EM; } }\n$x: 2Rem;";
    assert_eq!(
      fix(NAME, json!("lower"), scss, Syntax::Scss),
      "a { width: 10px; @media print { top: 1em; } }\n$x: 2rem;"
    );
    let upper = fix(NAME, json!("upper"), "a { width: 10px; }", Syntax::Css);
    assert_eq!(upper, "a { width: 10PX; }");
  }

  #[test]
  fn does_not_flag_hex_colours_or_unicode_ranges() {
    assert!(width("#97C1FF", Syntax::Scss).is_empty());
    let css = "@font-face { unicode-range: U+1D00-1DBF, u+2C60-2C7F; }";
    assert!(lint(NAME, json!("lower"), css, Syntax::Css).is_empty());
  }

  #[test]
  fn does_not_flag_var_custom_property_with_uppercase_letter() {
    // `var(--base0C)` — the `C` is part of the custom property name, not a unit
    assert!(
      width("var(--base0C)", Syntax::Css).is_empty(),
      "uppercase letter in a CSS custom property name must not be flagged as a unit"
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("lower")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("upper")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      ["Invalid option value \"sometimes\" for rule \"@stylistic/unit-case\""]
    );
  }
}
