use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "color-hex-case";

declare_rule! {
  /// Enforce lowercase hex colors (e.g. `#ABC` → `#abc`).
  ///
  /// Equivalent to Stylelint's `color-hex-case` rule: "lower" (the default)
  /// or "upper".
  pub struct ColorHexCase => ColorHexCaseCheck {
    name: NAME,
    description: "Enforce lowercase for hex colors",
    default_severity: Warning,
  }
}

/// `color-hex-case` as configured.
pub struct ColorHexCaseCheck {
  /// `"upper"` rather than `"lower"`.
  upper: bool,
}

impl ColorHexCaseCheck {
  /// Stylelint's option: `"lower"` or `"upper"`.  Gale reads no option as
  /// `"lower"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let case = options.primary_one_of(&["lower", "upper"]);
    options.finish()?;
    Ok(Self {
      upper: case == Some("upper"),
    })
  }
}

impl Check for ColorHexCaseCheck {
  /// Flags hex colors not in the configured case and fixes them.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let mut diags = Vec::new();
    for decl in &rule.declarations {
      let decl_start = decl.span.offset;
      let decl_end = decl_start + decl.span.length;
      let search_area = if decl_end <= ctx.source.len() && decl_start < decl_end {
        &ctx.source[decl_start..decl_end]
      } else {
        &decl.value
      };

      for (rel_offset, hex) in find_hex_colors(search_area) {
        let expected = if self.upper {
          hex.to_ascii_uppercase()
        } else {
          hex.to_ascii_lowercase()
        };
        if expected != hex {
          let abs_offset = if decl_end <= ctx.source.len() && decl_start < decl_end {
            decl_start + rel_offset
          } else {
            decl_start
          };
          diags.push(
            Diagnostic::new(NAME, format!("Expected \"{hex}\" to be \"{expected}\""))
              .span(Span::new(abs_offset, hex.len()))
              .fix(Fix::new(
                format!(
                  "Convert to {} \"{expected}\"",
                  if self.upper { "uppercase" } else { "lowercase" }
                ),
                vec![Edit::new(Span::new(abs_offset, hex.len()), &expected)],
              )),
          );
        }
      }
    }
    diags
  }
}

/// Scans for `#`-prefixed hex tokens, returning each with its offset into `value`.
fn find_hex_colors(value: &str) -> Vec<(usize, String)> {
  let mut colors = Vec::new();
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;
  while i < len {
    if bytes[i] == b'#' {
      let start = i;
      i += 1;
      while i < len && (bytes[i] as char).is_ascii_hexdigit() {
        i += 1;
      }
      if i > start + 1 {
        colors.push((start, value[start..i].to_string()));
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
  use gale_css_parser::{Declaration, StyleRule};

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
  fn reports_uppercase_hex() {
    let rule = configured(&ColorHexCase, None);
    let d = rule.check(&style_with_value("#FFFFFF"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ffffff"));
    assert!(d[0].fix.is_some());
  }

  #[test]
  fn allows_lowercase_hex() {
    let rule = configured(&ColorHexCase, None);
    let d = rule.check(&style_with_value("#abc"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_mixed_case_hex() {
    let rule = configured(&ColorHexCase, None);
    let d = rule.check(&style_with_value("#aBcDeF"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#abcdef"));
  }

  #[test]
  fn upper_reports_and_fixes_lowercase_digits() {
    let rule = configured(&ColorHexCase, serde_json::json!("upper"));
    assert!(rule.check(&style_with_value("#ABCDEF"), &ctx()).is_empty());
    let d = rule.check(&style_with_value("#aBc"), &ctx());
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "Expected \"#aBc\" to be \"#ABC\"");
    assert_eq!(d[0].fix.as_ref().unwrap().edits[0].new_text, "#ABC");
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("upper")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("title")),
      ["Invalid option value \"title\" for rule \"color-hex-case\""]
    );
  }
}
