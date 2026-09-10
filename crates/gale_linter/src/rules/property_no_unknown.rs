use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::data::is_known_property;
use crate::rule::{Rule, RuleContext};

/// Pattern matcher for ignoreProperties — supports plain strings and regex-like "/pattern/".
enum PropertyMatcher {
  Exact(String),
  Regex(regex::Regex),
}

impl PropertyMatcher {
  fn from_pattern(pattern: &str) -> Self {
    if pattern.starts_with('/') && pattern.ends_with('/') && pattern.len() > 2 {
      let re_str = &pattern[1..pattern.len() - 1];
      match regex::Regex::new(re_str) {
        Ok(re) => PropertyMatcher::Regex(re),
        Err(_) => PropertyMatcher::Exact(pattern.to_ascii_lowercase()),
      }
    } else {
      PropertyMatcher::Exact(pattern.to_ascii_lowercase())
    }
  }

  fn matches(&self, lower_prop: &str, original_prop: &str) -> bool {
    match self {
      PropertyMatcher::Exact(s) => lower_prop == s,
      PropertyMatcher::Regex(re) => re.is_match(original_prop) || re.is_match(lower_prop),
    }
  }
}

pub struct PropertyNoUnknown;

impl Rule for PropertyNoUnknown {
  fn name(&self) -> &'static str {
    "property-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    // Check for `ignoreProperties` option (may be in secondary options object).
    // Supports plain strings (exact match) and regex patterns like "/^mso-/".
    let secondary = ctx.secondary_options().or(ctx.options);
    let ignore_properties: Vec<PropertyMatcher> = secondary
      .and_then(|v| v.get("ignoreProperties"))
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .filter_map(|item| item.as_str().map(PropertyMatcher::from_pattern))
          .collect()
      })
      .unwrap_or_default();

    let mut diags = Vec::new();
    for decl in &rule.declarations {
      let prop = &decl.property;
      // Skip custom properties and vendor-prefixed
      if prop.starts_with("--") || prop.starts_with('-') {
        continue;
      }
      // Skip SCSS variable declarations ($var)
      if matches!(
        ctx.syntax,
        gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass
      ) && prop.starts_with('$')
      {
        continue;
      }
      // Skip properties containing SCSS interpolation #{...}
      if matches!(
        ctx.syntax,
        gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass
      ) && prop.contains("#{")
      {
        continue;
      }
      // Skip Less variable declarations (@var)
      if ctx.syntax == gale_css_parser::Syntax::Less && prop.starts_with('@') {
        continue;
      }
      // Skip explicitly ignored properties (exact match or regex pattern)
      let prop_lower = prop.to_ascii_lowercase();
      if ignore_properties
        .iter()
        .any(|m| m.matches(&prop_lower, prop))
      {
        continue;
      }
      if !is_known_property(prop) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected unknown property \"{prop}\""),
          )
          .severity(self.default_severity())
          .span(Span::new(decl.span.offset, decl.span.length)),
        );
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use gale_linter_test_helper::*;

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_unknown() {
    let rule = PropertyNoUnknown;
    let node = style_node("a", &[("colr", "red")]);
    let d = rule.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("colr"));
  }

  #[test]
  fn allows_known() {
    let rule = PropertyNoUnknown;
    let node = style_node("a", &[("color", "red")]);
    assert!(rule.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn skips_custom_and_vendor() {
    let rule = PropertyNoUnknown;
    let node = style_node("a", &[("--my-var", "1"), ("-webkit-appearance", "none")]);
    assert!(rule.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn ignore_properties_exact() {
    let rule = PropertyNoUnknown;
    let node = style_node("a", &[("mso-padding-alt", "0")]);
    let opts = serde_json::json!({ "ignoreProperties": ["mso-padding-alt"] });
    let c = RuleContext {
      options: Some(&opts),
      ..ctx()
    };
    assert!(rule.check(&node, &c).is_empty());
  }

  #[test]
  fn ignore_properties_regex_pattern() {
    let rule = PropertyNoUnknown;
    let node = style_node("a", &[("mso-padding-alt", "0"), ("mso-table-lspace", "0")]);
    let opts = serde_json::json!({ "ignoreProperties": ["/^mso-/"] });
    let c = RuleContext {
      options: Some(&opts),
      ..ctx()
    };
    assert!(rule.check(&node, &c).is_empty());
  }

  #[test]
  fn ignore_properties_regex_does_not_match_other() {
    let rule = PropertyNoUnknown;
    let node = style_node("a", &[("colr", "red")]);
    let opts = serde_json::json!({ "ignoreProperties": ["/^mso-/"] });
    let c = RuleContext {
      options: Some(&opts),
      ..ctx()
    };
    assert_eq!(rule.check(&node, &c).len(), 1);
  }

  mod gale_linter_test_helper {
    use gale_css_parser::*;
    pub fn style_node(sel: &str, props: &[(&str, &str)]) -> CssNode {
      CssNode::Style(StyleRule {
        selector: sel.to_string(),
        declarations: props
          .iter()
          .map(|(p, v)| Declaration {
            property: p.to_string(),
            value: v.to_string(),
            span: Span::new(0, 0),
            important: false,
          })
          .collect(),
        span: Span::new(0, 0),
        ..Default::default()
      })
    }
  }
}
