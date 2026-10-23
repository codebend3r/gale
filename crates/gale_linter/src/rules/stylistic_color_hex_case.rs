use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::hex_colors::find_hex_colors;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/color-hex-case";

declare_rule! {
  /// Specify lowercase or uppercase for hex colors (@stylistic/ version).
  ///
  /// Equivalent to `@stylistic/color-hex-case`. Supports both "lower" and "upper"
  /// primary options, unlike the core `color-hex-case` which only enforces lowercase.
  pub struct StylisticColorHexCase => StylisticColorHexCaseCheck {
    name: NAME,
    description: "Specify lowercase or uppercase for hex colors",
    default_severity: Warning,
  }
}

/// `@stylistic/color-hex-case` as configured.
pub struct StylisticColorHexCaseCheck {
  /// `"lower"` or `"upper"`.
  expected_case: &'static str,
}

impl StylisticColorHexCaseCheck {
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

impl Check for StylisticColorHexCaseCheck {
  /// Flags hex colors not in the configured case and offers the converted value
  /// as a fix.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let expected_case = self.expected_case;

    let mut diags = Vec::new();
    // The declarations of style rules, read whole: property through `;`.
    for decl in tree.walk_decls() {
      if !decl.in_style_rule() {
        continue;
      }
      let decl_start = decl.span().offset;
      let search_area = decl.source_text();

      for (rel_offset, hex) in find_hex_colors(search_area) {
        let is_wrong = match expected_case {
          "lower" => hex[1..].chars().any(|c| c.is_ascii_uppercase()),
          "upper" => hex[1..].chars().any(|c| c.is_ascii_lowercase()),
          _ => false,
        };

        if is_wrong {
          let fixed = match expected_case {
            "lower" => hex.to_ascii_lowercase(),
            "upper" => {
              // Keep the '#' as-is, uppercase the rest
              let mut s = String::with_capacity(hex.len());
              s.push('#');
              for c in hex[1..].chars() {
                s.extend(c.to_uppercase());
              }
              s
            }
            _ => continue,
          };
          let abs_offset = decl_start + rel_offset;
          diags.push(
            Diagnostic::new(NAME, format!("Expected \"{hex}\" to be \"{fixed}\""))
              .span(Span::new(abs_offset, hex.len()))
              .fix(Fix::new(
                format!("Convert to {expected_case}case"),
                vec![Edit::new(Span::new(abs_offset, hex.len()), &fixed)],
              )),
          );
        }
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  fn check(rule: &dyn Check, value: &str) -> Vec<Diagnostic> {
    let source = format!("a {{ color: {value}; }}");
    rule.check_source(&ctx_with_source(&source))
  }

  #[test]
  fn reports_uppercase_hex_default_lower() {
    let d = check(&*configured(&StylisticColorHexCase, None), "#FFFFFF");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ffffff"));
    assert_eq!(d[0].span, Span::new(11, 7));
  }

  #[test]
  fn allows_lowercase_hex() {
    let d = check(&*configured(&StylisticColorHexCase, None), "#abc");
    assert!(d.is_empty());
  }

  #[test]
  fn reports_mixed_case() {
    let d = check(&*configured(&StylisticColorHexCase, None), "#aBcDeF");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#abcdef"));
  }

  #[test]
  fn upper_reports_lowercase_hex() {
    let rule = configured(&StylisticColorHexCase, serde_json::json!("upper"));
    let d = check(&*rule, "#aBc");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ABC"));
  }

  #[test]
  fn only_lower_and_upper_are_options() {
    assert_eq!(
      invalid_options(NAME, serde_json::json!("Lower")),
      ["Invalid option value \"Lower\" for rule \"@stylistic/color-hex-case\""]
    );
    assert!(invalid_options(NAME, serde_json::json!("upper")).is_empty());
  }
}
