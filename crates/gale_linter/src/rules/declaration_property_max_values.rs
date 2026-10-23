use std::collections::HashMap;

use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::value_parser::{self, NodeKind};
use gale_diagnostics::Diagnostic;
use serde_json::{Map, Value};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting, is_object_of_non_negative_integers};
use crate::pattern;
use crate::rule::{Check, RuleContext, declare_rule};
use crate::standard_syntax::is_standard_syntax_value;

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
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    if self.exact.is_empty() && self.patterns.is_empty() {
      return vec![];
    }

    let mut diags = Vec::new();
    for decl in tree.walk_decls() {
      let prop_lower = decl.prop().to_ascii_lowercase();

      // v17: check property as-is, no vendor prefix stripping
      let max = self.limit_for(&prop_lower);
      let Some(max_values) = max else {
        continue;
      };

      let Some(value_count) = count_values(decl.value()) else {
        continue;
      };
      if value_count > max_values {
        diags.push(
          Diagnostic::new(
            NAME,
            format!(
              "Expected no more than {max_values} value(s) for property \
                             \"{}\", found {value_count}",
              decl.prop(),
            ),
          )
          .span(decl.span()),
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

/// How many values Stylelint counts in `value`: the words, functions and
/// strings at its top level, so that `1px solid red` holds three, `a, b`
/// two and `12px/0px sans-serif` three, while a function's arguments are
/// part of it.
///
/// `None` where Stylelint counts nothing: a value with no whitespace or
/// comma (`mayIncludeRegexes.multipleValues`), or one in preprocessor
/// syntax (`isStandardSyntaxValue`).
fn count_values(value: &str) -> Option<usize> {
  if !value.contains(|c: char| c.is_whitespace() || c == ',') || !is_standard_syntax_value(value) {
    return None;
  }
  let count = value_parser::parse(value)
    .iter()
    .filter(|node| {
      matches!(
        node.kind,
        NodeKind::Word | NodeKind::Function | NodeKind::String
      )
    })
    .count();
  Some(count)
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_diagnostics::Span;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  fn style_decl(prop: &str, val: &str) -> String {
    format!("a {{ {prop}: {val}; }}")
  }

  #[test]
  fn flags_excess_values() {
    let opts = serde_json::json!({"border": 2});
    let rule = configured(&DeclarationPropertyMaxValues, opts.clone());
    let d = rule.check_source(&ctx_with_source(&style_decl("border", "1px solid red")));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("no more than 2"));
    assert_eq!(d[0].span, Span::new(4, 22));
  }

  #[test]
  fn allows_within_limit() {
    let opts = serde_json::json!({"border": 3});
    let rule = configured(&DeclarationPropertyMaxValues, opts.clone());
    let d = rule.check_source(&ctx_with_source(&style_decl("border", "1px solid red")));
    assert!(d.is_empty());
  }

  #[test]
  fn does_not_match_vendor_prefixed_property() {
    // v17: "border" limit should NOT match "-webkit-border"
    let opts = serde_json::json!({"border": 1});
    let rule = configured(&DeclarationPropertyMaxValues, opts.clone());
    let d = rule.check_source(&ctx_with_source(&style_decl(
      "-webkit-border",
      "1px solid red",
    )));
    assert!(
      d.is_empty(),
      "v17: property limit should not match vendor-prefixed equivalent"
    );
  }

  #[test]
  fn matches_vendor_prefixed_when_explicitly_specified() {
    let opts = serde_json::json!({"-webkit-border": 1});
    let rule = configured(&DeclarationPropertyMaxValues, opts.clone());
    let d = rule.check_source(&ctx_with_source(&style_decl(
      "-webkit-border",
      "1px solid red",
    )));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn function_args_count_as_one_value() {
    let opts = serde_json::json!({"background": 1});
    let rule = configured(&DeclarationPropertyMaxValues, opts.clone());
    let d = rule.check_source(&ctx_with_source(&style_decl("background", "rgb(0, 0, 0)")));
    assert!(
      d.is_empty(),
      "function args should not be counted as separate values"
    );
  }

  #[test]
  fn reads_the_value_and_property_as_written() {
    let rule = configured(
      &DeclarationPropertyMaxValues,
      serde_json::json!({"margin": 1}),
    );
    // The CSS parser used to collapse `0 0 0 0` to `0` and lower-case the
    // property.
    let d = rule.check_source(&ctx_with_source(&style_decl("MARGIN", "0 0 0 0")));
    assert_eq!(d.len(), 1);
    assert_eq!(
      d[0].message,
      "Expected no more than 1 value(s) for property \"MARGIN\", found 4"
    );
  }

  #[test]
  fn count_values_basic() {
    assert_eq!(count_values("1px solid red"), Some(3));
    assert_eq!(count_values("rgb(0, 0, 0) 1px"), Some(2));
    // As Stylelint counts them: the words, functions and strings at the
    // top level.
    assert_eq!(count_values("a, b"), Some(2));
    assert_eq!(count_values("12px/0px sans-serif"), Some(3));
    assert_eq!(count_values("\"a b\" c"), Some(2));
    // Stylelint counts no lone value, and no preprocessor value.
    assert_eq!(count_values("1px"), None);
    assert_eq!(count_values(""), None);
    assert_eq!(count_values("$a $b"), None);
  }

  #[test]
  fn regex_keys_match_by_search() {
    let rule = configured(
      &DeclarationPropertyMaxValues,
      serde_json::json!([{ "/^margin/": 1 }]),
    );
    let check = |prop, value| rule.check_source(&ctx_with_source(&style_decl(prop, value)));
    assert_eq!(check("margin-top", "1px 2px").len(), 1);
    assert!(check("padding", "1px 2px").is_empty());
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
