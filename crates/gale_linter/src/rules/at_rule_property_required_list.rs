use std::collections::HashMap;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};
use serde_json::{Map, Value};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting, is_object_of_string_lists};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "at-rule-property-required-list";

declare_rule! {
  /// Require specific properties for at-rules.
  ///
  /// Options: an object mapping at-rule names (lowercase, without `@`) to arrays
  /// of required property names.
  ///
  /// Example config:
  /// ```json
  /// { "font-face": ["font-family", "font-style"] }
  /// ```
  ///
  /// Equivalent to Stylelint's `at-rule-property-required-list` rule.
  pub struct AtRulePropertyRequiredList => AtRulePropertyRequiredListCheck {
    name: NAME,
    description: "Specify a list of required properties for at-rules",
    default_severity: Warning,
    primary_option_object: true,
  }
}

/// `at-rule-property-required-list` as configured.
pub struct AtRulePropertyRequiredListCheck {
  /// Lowercase at-rule names to the lowercase properties each requires.
  required: HashMap<String, Vec<String>>,
}

impl AtRulePropertyRequiredListCheck {
  /// Stylelint's option: an object whose values are each a property name or
  /// a list of them (`validateObjectWithArrayProps(isString)`).
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let required = match options.primary_object() {
      Some(map) => {
        let object = Value::Object(map.clone());
        if is_object_of_string_lists(&object) {
          parse_options(map)
        } else {
          options.report(InvalidOption::value(NAME, &object));
          HashMap::new()
        }
      }
      None => HashMap::new(),
    };
    options.finish()?;
    Ok(Self { required })
  }
}

impl Check for AtRulePropertyRequiredListCheck {
  /// Flags an at-rule that is missing any property the config requires for it.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at_rule) = node else {
      return vec![];
    };

    let name_lower = at_rule.name.to_ascii_lowercase();
    let required_props = match self.required.get(name_lower.as_str()) {
      Some(props) => props,
      None => return vec![],
    };

    // Collect declared properties from the at-rule's children.
    let declared: Vec<String> = at_rule
      .children
      .iter()
      .filter_map(|child| {
        if let CssNode::Declaration(decl) = child {
          Some(decl.property.to_ascii_lowercase())
        } else {
          None
        }
      })
      .collect();

    let mut diags = Vec::new();
    for req in required_props {
      if !declared.iter().any(|d| d == req) {
        diags.push(
          Diagnostic::new(
            NAME,
            format!(
              "Expected property \"{req}\" in at-rule \"@{name}\"",
              name = at_rule.name,
            ),
          )
          .span(Span::new(at_rule.span.offset, at_rule.span.length)),
        );
      }
    }
    diags
  }
}

/// The rule's option `{ "at-rule-name": ["prop1", "prop2"] }`, names
/// lowercased; a single name stands for a list of one.
fn parse_options(map: &Map<String, Value>) -> HashMap<String, Vec<String>> {
  map
    .iter()
    .map(|(key, value)| {
      let props = match value {
        Value::Array(items) => items
          .iter()
          .filter_map(|v| v.as_str().map(str::to_ascii_lowercase))
          .collect(),
        other => other
          .as_str()
          .map(str::to_ascii_lowercase)
          .into_iter()
          .collect(),
      };
      (key.to_ascii_lowercase(), props)
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Declaration, Span as ParserSpan};

  use crate::testing::{configured, ctx, invalid_options};

  fn at_rule_with_decls(name: &str, decls: Vec<(&str, &str)>) -> CssNode {
    let children = decls
      .into_iter()
      .map(|(prop, val)| {
        CssNode::Declaration(Declaration {
          property: prop.to_string(),
          value: val.to_string(),
          span: ParserSpan::new(0, 10),
          important: false,
        })
      })
      .collect();
    CssNode::AtRule(AtRule {
      name: name.to_string(),
      params: String::new(),
      span: ParserSpan::new(0, 20),
      children,
    })
  }

  #[test]
  fn reports_missing_required_property() {
    let opts = serde_json::json!({ "font-face": ["font-family", "font-style"] });
    let rule = configured(&AtRulePropertyRequiredList, opts.clone());
    let c = ctx();

    let node = at_rule_with_decls("font-face", vec![("font-family", "Arial")]);
    let d = rule.check(&node, &c);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("font-style"));
  }

  #[test]
  fn allows_when_all_properties_present() {
    let opts = serde_json::json!({ "font-face": ["font-family", "font-style"] });
    let rule = configured(&AtRulePropertyRequiredList, opts.clone());
    let c = ctx();

    let node = at_rule_with_decls(
      "font-face",
      vec![("font-family", "Arial"), ("font-style", "normal")],
    );
    let d = rule.check(&node, &c);
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_unmatched_at_rules() {
    let opts = serde_json::json!({ "font-face": ["font-family"] });
    let rule = configured(&AtRulePropertyRequiredList, opts.clone());
    let c = ctx();

    let node = at_rule_with_decls("media", vec![]);
    let d = rule.check(&node, &c);
    assert!(d.is_empty());
  }

  #[test]
  fn returns_empty_when_no_options() {
    let rule = configured(&AtRulePropertyRequiredList, None);
    let ctx = ctx();
    let node = at_rule_with_decls("font-face", vec![]);
    let d = rule.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn a_single_name_is_a_list_of_one() {
    let rule = configured(
      &AtRulePropertyRequiredList,
      serde_json::json!({ "font-face": "font-display" }),
    );
    let node = at_rule_with_decls("font-face", vec![("font-family", "Arial")]);
    let d = rule.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("font-display"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([{ "font-face": ["font-family"], "page": "margin" }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("font-face")),
      ["Invalid option value \"font-face\" for rule \"at-rule-property-required-list\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!({ "font-face": [1] })),
      ["Invalid option value \"{\"font-face\":[1]}\" for rule \"at-rule-property-required-list\""]
    );
  }
}
