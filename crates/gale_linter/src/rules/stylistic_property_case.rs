use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

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
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let expected_case = self.expected_case;

    let decls: Vec<_> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    let mut diags = Vec::new();
    for decl in decls {
      let prop = &decl.property;
      // Skip custom properties (--*) and SCSS variables ($*)
      if prop.starts_with("--") || prop.starts_with('$') {
        continue;
      }
      // Skip vendor-prefixed properties for the case check
      let check_part = if prop.starts_with('-') {
        // e.g. -webkit-transform -> check "transform" part
        if let Some(idx) = prop[1..].find('-') {
          &prop[idx + 2..]
        } else {
          prop.as_str()
        }
      } else {
        prop.as_str()
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
            .span(Span::new(decl.span.offset, prop.len()))
            .fix(Fix::new(
              format!("Convert to {expected_case}case"),
              vec![Edit::new(Span::new(decl.span.offset, prop.len()), &fixed)],
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
  use gale_css_parser::{Declaration, StyleRule};

  use crate::testing::{configured, ctx, invalid_options};

  fn style_with_prop(prop: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: "red".to_string(),
        span: Span::new(0, prop.len() + 6),
        important: false,
      }],
      span: Span::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_uppercase_property() {
    let rule = configured(&StylisticPropertyCase, None);
    let ctx = ctx();
    let d = rule.check(&style_with_prop("Color"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("color"));
  }

  #[test]
  fn allows_lowercase_property() {
    let rule = configured(&StylisticPropertyCase, None);
    let ctx = ctx();
    let d = rule.check(&style_with_prop("color"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn skips_custom_properties() {
    let rule = configured(&StylisticPropertyCase, None);
    let ctx = ctx();
    let d = rule.check(&style_with_prop("--myColor"), &ctx);
    assert!(d.is_empty());
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
