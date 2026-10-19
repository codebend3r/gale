use gale_css_parser::CssNode;
use gale_diagnostics::Diagnostic;

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
  /// Flags hex colors whose digit count is not 3, 4, 6 or 8.
  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let mut diags = Vec::new();
    for decl in &rule.declarations {
      for hex in hex_candidates(&decl.value) {
        if !is_valid_hex(&hex) {
          diags.push(
            Diagnostic::new(NAME, format!("Unexpected invalid hex color \"{hex}\""))
              .span(decl.span),
          );
        }
      }
    }
    diags
  }
}

/// The words after a `#` in a value: hex colors, valid or not.  Unlike
/// [`crate::hex_colors::find_hex_colors`] it reads any letters and digits,
/// as an invalid color is what this rule looks for.
/// Skips:
/// - Content inside `url(…)` (URLs may contain `#fragment` references)
/// - Content after `//` (SCSS line comments that may leak into the value)
/// - `#{}` SCSS interpolation
fn hex_candidates(value: &str) -> Vec<String> {
  let mut colors = Vec::new();
  let chars: Vec<char> = value.chars().collect();
  let len = chars.len();

  let mut i = 0;
  while i < len {
    // Skip SCSS line comments (// to end of line)
    if i + 1 < len && chars[i] == '/' && chars[i + 1] == '/' {
      while i < len && chars[i] != '\n' {
        i += 1;
      }
      continue;
    }

    // Skip url(…)
    if i + 3 < len
      && chars[i] == 'u'
      && chars[i + 1] == 'r'
      && chars[i + 2] == 'l'
      && chars[i + 3] == '('
    {
      i += 4;
      let mut depth = 1;
      while i < len && depth > 0 {
        match chars[i] {
          '(' => depth += 1,
          ')' => depth -= 1,
          _ => {}
        }
        i += 1;
      }
      continue;
    }

    if chars[i] == '#' {
      // Skip SCSS interpolation #{…}
      if i + 1 < len && chars[i + 1] == '{' {
        i += 2;
        let mut depth = 1;
        while i < len && depth > 0 {
          match chars[i] {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
          }
          i += 1;
        }
        continue;
      }

      let start = i;
      i += 1;
      while i < len && chars[i].is_ascii_alphanumeric() {
        i += 1;
      }
      if i > start + 1 {
        let hex: String = chars[start..i].iter().collect();
        colors.push(hex);
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
  use gale_css_parser::{CssNode, Declaration, StyleRule};
  use gale_diagnostics::Span;

  use crate::testing::{configured, ctx, invalid_options};

  fn style_with_value(val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: val.to_string(),
        span: Span::new(0, 0),
        important: false,
      }],
      span: Span::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_invalid_hex_chars() {
    let rule = configured(&ColorNoInvalidHex, None);
    let d = rule.check(&style_with_value("#gg0000"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#gg0000"));
  }

  #[test]
  fn reports_invalid_hex_length() {
    let rule = configured(&ColorNoInvalidHex, None);
    let d = rule.check(&style_with_value("#12345"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_valid_hex() {
    let rule = configured(&ColorNoInvalidHex, None);
    assert!(rule.check(&style_with_value("#fff"), &ctx()).is_empty());
    assert!(rule.check(&style_with_value("#ff00ff"), &ctx()).is_empty());
    assert!(
      rule
        .check(&style_with_value("#ff00ff80"), &ctx())
        .is_empty()
    );
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
