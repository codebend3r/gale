use std::collections::HashMap;

use gale_css_parser::postcss_tree::PostcssTree;
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
  /// Flags an at-rule with a block that is missing any property the config
  /// requires for it, at its `@name` word as Stylelint reports it.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for at_rule in tree.walk_at_rules() {
      if !at_rule.has_block() || !at_rule.is_standard_syntax() {
        continue;
      }
      let name = at_rule.name();
      let Some(required_props) = self.required.get(name.to_ascii_lowercase().as_str()) else {
        continue;
      };

      // The properties declared directly in the block.
      let declared: Vec<String> = at_rule
        .child_decls()
        .map(|decl| decl.prop().to_ascii_lowercase())
        .collect();

      for req in required_props {
        if !declared.iter().any(|d| d == req) {
          diags.push(
            Diagnostic::new(
              NAME,
              format!("Expected property \"{req}\" in at-rule \"@{name}\""),
            )
            .span(Span::new(at_rule.start, name.len() + 1)),
          );
        }
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
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options, lint};

  #[test]
  fn reports_missing_required_property_at_the_name() {
    let opts = json!({ "font-face": ["font-family", "font-style"] });
    for css in [
      "a {}\n@font-face { font-family: Arial; }",
      "a {}\n@FONT-FACE { FONT-FAMILY: Arial; }",
    ] {
      let d = lint(NAME, opts.clone(), css, Syntax::Css);
      assert_eq!(d.len(), 1, "{css}");
      assert!(d[0].message.contains("font-style"));
      assert_eq!((d[0].span.offset, d[0].span.length), (5, 10));
    }
  }

  #[test]
  fn allows_when_all_properties_present() {
    let opts = json!({ "font-face": ["font-family", "font-style"] });
    let css = "@font-face { font-family: Arial; font-style: normal; }";
    assert!(lint(NAME, opts, css, Syntax::Css).is_empty());
    // `@page` and `@property` keep their descriptors too.
    let opts = json!({ "page": ["margin"], "property": ["syntax"] });
    let css = "@page { margin: 1cm; }\n@property --x { syntax: '*'; inherits: false; }";
    assert!(lint(NAME, opts, css, Syntax::Css).is_empty());
  }

  #[test]
  fn ignores_unmatched_and_blockless_at_rules() {
    let opts = json!({ "font-face": ["font-family"] });
    assert!(lint(NAME, opts, "@media screen {}\n@font-face x;", Syntax::Css).is_empty());
  }

  #[test]
  fn returns_empty_when_no_options() {
    let rule = configured(&AtRulePropertyRequiredList, None);
    assert!(
      rule
        .check_source(&ctx_with_source("@font-face {}"))
        .is_empty()
    );
  }

  #[test]
  fn a_single_name_is_a_list_of_one() {
    let opts = json!({ "font-face": "font-display" });
    let d = lint(
      NAME,
      opts,
      "@font-face { font-family: Arial; }",
      Syntax::Css,
    );
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
