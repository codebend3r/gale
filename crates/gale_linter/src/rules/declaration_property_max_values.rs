use std::collections::HashMap;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};
use serde_json::{Map, Value};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting, is_object_of_non_negative_integers};
use crate::pattern;
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "declaration-property-max-values";

declare_rule! {
  /// Limit the number of values in a declaration for specific properties.
  ///
  /// Options: an object mapping property names (or patterns) to maximum
  /// allowed value counts.
  /// Example: `{"border": 2, "/^margin/": 1}`
  ///
  /// v17: Properties are checked as-is (no vendor prefix stripping).
  /// E.g. `{ "border": 2 }` does NOT match `-webkit-border`.
  ///
  /// Equivalent to Stylelint's `declaration-property-max-values` rule.
  pub struct DeclarationPropertyMaxValues => DeclarationPropertyMaxValuesCheck {
    name: NAME,
    description: "Limit the number of values for properties",
    default_severity: Warning,
    primary_option_object: true,
  }
}

/// `declaration-property-max-values` as configured.
pub struct DeclarationPropertyMaxValuesCheck {
  /// The limits keyed by lowercase property name.
  exact: HashMap<String, usize>,
  /// The limits keyed by a `/regex/` literal, in the order the config
  /// lists them.
  patterns: Vec<(Matchers, usize)>,
}

impl DeclarationPropertyMaxValuesCheck {
  /// Stylelint's option: an object from property names (or `/regex/`
  /// literals) to whole numbers of zero or more
  /// (`validateObjectWithProps(isNonNegativeInteger)`).
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let limits = match options.primary_object() {
      Some(map) => {
        let object = Value::Object(map.clone());
        if is_object_of_non_negative_integers(&object) {
          parse_options(map)
        } else {
          options.report(InvalidOption::value(NAME, &object));
          Vec::new()
        }
      }
      None => Vec::new(),
    };
    options.finish()?;
    let mut exact = HashMap::new();
    let mut patterns = Vec::new();
    for (key, max) in limits {
      if pattern::split_literal(&key).is_some() {
        patterns.push((Matchers::new(&[key]), max));
      } else {
        exact.insert(key, max);
      }
    }
    Ok(Self { exact, patterns })
  }

  /// The limit for a lowercase property: an exact entry first, then the
  /// first `/regex/` entry that matches.
  fn limit_for(&self, prop: &str) -> Option<usize> {
    self.exact.get(prop).copied().or_else(|| {
      self
        .patterns
        .iter()
        .find(|(matcher, _)| matcher.matches(prop))
        .map(|&(_, max)| max)
    })
  }
}

impl Check for DeclarationPropertyMaxValuesCheck {
  /// Flags declarations holding more values than the config allows for that
  /// property. Property names are matched as written, without prefix stripping.
  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    if self.exact.is_empty() && self.patterns.is_empty() {
      return vec![];
    }

    let decls: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    let mut diags = Vec::new();
    for decl in decls {
      let prop_lower = decl.property.to_ascii_lowercase();

      // v17: check property as-is, no vendor prefix stripping
      let max = self.limit_for(&prop_lower);
      let Some(max_values) = max else {
        continue;
      };

      let value_count = count_values(&decl.value);
      if value_count > max_values {
        diags.push(
          Diagnostic::new(
            NAME,
            format!(
              "Expected no more than {max_values} value(s) for property \
                             \"{}\", found {value_count}",
              decl.property,
            ),
          )
          .span(Span::new(decl.span.offset, decl.span.length)),
        );
      }
    }
    diags
  }
}

/// The limits the option sets, with lowercase property names.
fn parse_options(map: &Map<String, Value>) -> Vec<(String, usize)> {
  map
    .iter()
    .filter_map(|(prop, max)| Some((prop.to_ascii_lowercase(), max.as_f64()? as usize)))
    .collect()
}

/// Count the number of space-separated values in a CSS value string.
/// Respects parentheses (function arguments are not counted as separate values).
fn count_values(value: &str) -> usize {
  let trimmed = value.trim();
  if trimmed.is_empty() {
    return 0;
  }

  let mut count = 0;
  let mut depth: usize = 0;
  let mut in_value = false;

  for ch in trimmed.chars() {
    match ch {
      '(' => {
        depth += 1;
        in_value = true;
      }
      ')' => {
        depth = depth.saturating_sub(1);
      }
      c if c.is_ascii_whitespace() && depth == 0 => {
        if in_value {
          count += 1;
          in_value = false;
        }
      }
      _ => {
        in_value = true;
      }
    }
  }
  if in_value {
    count += 1;
  }
  count
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx, invalid_options};

  fn style_decl(prop: &str, val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, prop.len() + val.len() + 2),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn flags_excess_values() {
    let opts = serde_json::json!({"border": 2});
    let rule = configured(&DeclarationPropertyMaxValues, opts.clone());
    let d = rule.check(&style_decl("border", "1px solid red"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("no more than 2"));
  }

  #[test]
  fn allows_within_limit() {
    let opts = serde_json::json!({"border": 3});
    let rule = configured(&DeclarationPropertyMaxValues, opts.clone());
    let d = rule.check(&style_decl("border", "1px solid red"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn does_not_match_vendor_prefixed_property() {
    // v17: "border" limit should NOT match "-webkit-border"
    let opts = serde_json::json!({"border": 1});
    let rule = configured(&DeclarationPropertyMaxValues, opts.clone());
    let d = rule.check(&style_decl("-webkit-border", "1px solid red"), &ctx());
    assert!(
      d.is_empty(),
      "v17: property limit should not match vendor-prefixed equivalent"
    );
  }

  #[test]
  fn matches_vendor_prefixed_when_explicitly_specified() {
    let opts = serde_json::json!({"-webkit-border": 1});
    let rule = configured(&DeclarationPropertyMaxValues, opts.clone());
    let d = rule.check(&style_decl("-webkit-border", "1px solid red"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn function_args_count_as_one_value() {
    let opts = serde_json::json!({"background": 1});
    let rule = configured(&DeclarationPropertyMaxValues, opts.clone());
    let d = rule.check(&style_decl("background", "rgb(0, 0, 0)"), &ctx());
    assert!(
      d.is_empty(),
      "function args should not be counted as separate values"
    );
  }

  #[test]
  fn count_values_basic() {
    assert_eq!(count_values("1px solid red"), 3);
    assert_eq!(count_values("1px"), 1);
    assert_eq!(count_values("rgb(0, 0, 0) 1px"), 2);
    assert_eq!(count_values(""), 0);
  }

  #[test]
  fn regex_keys_match_by_search() {
    let rule = configured(
      &DeclarationPropertyMaxValues,
      serde_json::json!([{ "/^margin/": 1 }]),
    );
    let ctx = ctx();
    assert_eq!(
      rule.check(&style_decl("margin-top", "1px 2px"), &ctx).len(),
      1
    );
    assert!(
      rule
        .check(&style_decl("padding", "1px 2px"), &ctx)
        .is_empty()
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!({ "border": 2, "/^margin/": 0 })).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!({ "border": -1 })),
      ["Invalid option value \"{\"border\":-1}\" for rule \"declaration-property-max-values\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("border")),
      ["Invalid option value \"border\" for rule \"declaration-property-max-values\""]
    );
  }
}
