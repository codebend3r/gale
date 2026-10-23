use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::hex_colors::is_valid_hex;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "color-no-invalid-hex";

declare_rule! {
  pub struct ColorNoInvalidHex => ColorNoInvalidHexCheck {
    name: NAME,
    description: "Disallow invalid hex colors",
    default_severity: Error,
  }
}

/// `color-no-invalid-hex`, which takes no options.
pub struct ColorNoInvalidHexCheck;

impl ColorNoInvalidHexCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ColorNoInvalidHexCheck {
  /// Flags hex colors whose digit count is not 3, 4, 6 or 8 in the
  /// declarations of style rules, at the color, as Stylelint reports it.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for decl in tree.walk_decls() {
      if !decl.in_style_rule() {
        continue;
      }
      let value = decl.raw_value();
      for (offset, hex) in hex_candidates(&value) {
        if !is_valid_hex(hex) {
          diags.push(
            Diagnostic::new(NAME, format!("Unexpected invalid hex color \"{hex}\""))
              .span(Span::new(decl.value_start() + offset, hex.len())),
          );
        }
      }
    }
    diags
  }
}

/// The words after a `#` in a value, with their offsets: hex colors, valid
/// or not.  Unlike [`crate::hex_colors::find_hex_colors`] it reads any
/// letters and digits, as an invalid color is what this rule looks for.
/// Skips:
/// - Content inside `url(…)` (URLs may contain `#fragment` references)
/// - Comments: `/* … */`, and `//` to the end of the line (SCSS line
///   comments that may leak into the value)
/// - `#{}` SCSS interpolation
fn hex_candidates(value: &str) -> Vec<(usize, &str)> {
  let mut colors = Vec::new();
  let bytes = value.as_bytes();
  let len = bytes.len();

  let mut i = 0;
  while i < len {
    // Skip SCSS line comments (// to end of line)
    if bytes[i..].starts_with(b"//") {
      while i < len && bytes[i] != b'\n' {
        i += 1;
      }
      continue;
    }

    // Skip block comments, which value-parser reads as comment nodes.
    if bytes[i..].starts_with(b"/*") {
      i = value[i + 2..].find("*/").map_or(len, |end| i + 2 + end + 2);
      continue;
    }

    // Skip url(…)
    if bytes[i..].starts_with(b"url(") {
      i += 4;
      let mut depth = 1;
      while i < len && depth > 0 {
        match bytes[i] {
          b'(' => depth += 1,
          b')' => depth -= 1,
          _ => {}
        }
        i += 1;
      }
      continue;
    }

    if bytes[i] == b'#' {
      // Skip SCSS interpolation #{…}
      if i + 1 < len && bytes[i + 1] == b'{' {
        i += 2;
        let mut depth = 1;
        while i < len && depth > 0 {
          match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
          }
          i += 1;
        }
        continue;
      }

      let start = i;
      i += 1;
      while i < len && bytes[i].is_ascii_alphanumeric() {
        i += 1;
      }
      if i > start + 1 {
        colors.push((start, &value[start..i]));
      }
    } else {
      i += 1;
    }
  }

  colors
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  fn check(value: &str) -> Vec<Diagnostic> {
    let source = format!("a {{ color: {value}; }}");
    configured(&ColorNoInvalidHex, None).check_source(&ctx_with_source(&source))
  }

  #[test]
  fn reports_invalid_hex_chars() {
    let d = check("#gg0000");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#gg0000"));
    // At the color, as Stylelint reports it.
    assert_eq!(d[0].span, Span::new(11, 7));
  }

  #[test]
  fn reports_invalid_hex_length() {
    assert_eq!(check("#12345").len(), 1);
  }

  #[test]
  fn allows_valid_hex() {
    assert!(check("#fff").is_empty());
    assert!(check("#ff00ff").is_empty());
    assert!(check("#ff00ff80").is_empty());
  }

  #[test]
  fn skips_comments_and_urls() {
    assert!(check("red /* #zz */").is_empty());
    assert!(check("url(a.svg#zz)").is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"color-no-invalid-hex\""]
    );
  }
}
