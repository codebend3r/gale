use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::standard_syntax::is_standard_syntax_property;

const NAME: &str = "@stylistic/property-case";

declare_rule! {
  /// Specify lowercase or uppercase for properties.
  ///
  /// Equivalent to `@stylistic/property-case`.
  pub struct StylisticPropertyCase => StylisticPropertyCaseCheck {
    name: NAME,
    description: "Specify lowercase or uppercase for properties",
    default_severity: Warning,
  }
}

/// `@stylistic/property-case` as configured.
pub struct StylisticPropertyCaseCheck {
  /// The primary option: `"lower"` or `"upper"`.
  expected_case: &'static str,
}

impl StylisticPropertyCaseCheck {
  /// The plugin's options: `"lower"` or `"upper"`, then
  /// `ignoreSelectors`.  Gale reads no option as `"lower"`.  Gale accepts but
  /// does not yet apply the secondary options.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expected_case = options.primary_one_of(&["lower", "upper"]);
    options.strings("ignoreSelectors");
    options.finish()?;
    Ok(Self {
      expected_case: expected_case.unwrap_or("lower"),
    })
  }
}

impl Check for StylisticPropertyCaseCheck {
  /// Flags property names not in the configured case.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let expected_case = self.expected_case;

    let mut diags = Vec::new();
    for decl in tree.walk_decls() {
      let (prop, prop_start) = (decl.prop(), decl.prop_start());
      // Skip custom properties (--*) and, as Stylelint's
      // `isStandardSyntaxProperty` does, Sass variables (`$a`, `ns.$a`),
      // Less variables and merges, and interpolated names.
      if prop.starts_with("--") || !is_standard_syntax_property(prop) {
        continue;
      }
      // Skip vendor-prefixed properties for the case check
      let check_part = if prop.starts_with('-') {
        // e.g. -webkit-transform -> check "transform" part
        if let Some(idx) = prop[1..].find('-') {
          &prop[idx + 2..]
        } else {
          prop
        }
      } else {
        prop
      };

      let is_wrong = match expected_case {
        "lower" => check_part.chars().any(|c| c.is_ascii_uppercase()),
        "upper" => check_part.chars().any(|c| c.is_ascii_lowercase()),
        _ => false,
      };

      if is_wrong {
        let fixed = match expected_case {
          "lower" => prop.to_ascii_lowercase(),
          "upper" => prop.to_ascii_uppercase(),
          _ => continue,
        };
        diags.push(
          Diagnostic::new(NAME, format!("Expected \"{prop}\" to be \"{fixed}\""))
            .span(Span::new(prop_start, prop.len()))
            .fix(Fix::new(
              format!("Convert to {expected_case}case"),
              vec![Edit::new(Span::new(prop_start, prop.len()), &fixed)],
            )),
        );
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  fn style_with_prop(prop: &str) -> String {
    format!("a {{ {prop}: red; }}")
  }

  #[test]
  fn reports_uppercase_property() {
    let rule = configured(&StylisticPropertyCase, None);
    let d = rule.check_source(&ctx_with_source(&style_with_prop("Color")));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("color"));
    assert_eq!(d[0].span, Span::new(4, 5));
    // Known properties too, which the CSS parser used to lower-case.
    let d = rule.check_source(&ctx_with_source(&style_with_prop("COLOR")));
    assert_eq!(d[0].message, "Expected \"COLOR\" to be \"color\"");
  }

  #[test]
  fn allows_lowercase_property() {
    let rule = configured(&StylisticPropertyCase, None);
    let d = rule.check_source(&ctx_with_source(&style_with_prop("color")));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_custom_properties() {
    let rule = configured(&StylisticPropertyCase, None);
    let d = rule.check_source(&ctx_with_source(&style_with_prop("--myColor")));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_preprocessor_properties() {
    let rule = configured(&StylisticPropertyCase, serde_json::json!("upper"));
    for source in [
      "a { #{$side}: 0; }",
      "a { #{$side}-width: 0; }",
      "mat.$theme-compat: true;",
      "$Var: 1;",
    ] {
      let ctx = crate::testing::scss_ctx_with_source(source);
      assert!(rule.check_source(&ctx).is_empty(), "{source}");
    }
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("lower")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("upper")).is_empty());
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["lower", {"ignoreSelectors": ["/^my-/", "url"]}])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      ["Invalid option value \"sometimes\" for rule \"@stylistic/property-case\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["lower", {"ignoreSelectorsX": ["url"]}])
      ),
      ["Invalid option name \"ignoreSelectorsX\" for rule \"@stylistic/property-case\""]
    );
  }
}
