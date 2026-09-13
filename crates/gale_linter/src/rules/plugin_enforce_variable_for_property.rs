use std::collections::HashMap;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

/// Enforce design token/variable usage for specified CSS properties.
///
/// A configurable meta-rule that checks whether declaration values match
/// allowed variable/token patterns. Covers the "variable enforcement" pattern
/// seen in Primer, Carbon, and Spectrum CSS custom plugins.
///
/// Options (secondary): an object with a `properties` key mapping CSS property
/// names to arrays of allowed value patterns (exact strings or `/regex/`).
///
/// Example config:
/// ```json
/// ["plugin/enforce-variable-for-property", [true, {
///   "properties": {
///     "color": ["/^\\$text-/", "inherit", "currentColor", "transparent"],
///     "background-color": ["/^\\$bg-/", "inherit", "transparent"]
///   }
/// }]]
/// ```
pub struct PluginEnforceVariableForProperty;

/// A compiled pattern — either a literal string match or a regex.
enum Pattern {
  Exact(String),
  Regex(Regex),
}

impl Pattern {
  fn from_str(s: &str) -> Self {
    // Patterns wrapped in `/` are treated as regexes.
    if let Some(inner) = s.strip_prefix('/').and_then(|s| {
      // Handle optional flags like /pattern/i
      if let Some(pos) = s.rfind('/') {
        Some((&s[..pos], &s[pos + 1..]))
      } else {
        None
      }
    }) {
      let (pattern, flags) = inner;
      let regex_str = if flags.contains('i') {
        format!("(?i){}", pattern)
      } else {
        pattern.to_string()
      };
      match Regex::new(&regex_str) {
        Ok(re) => Pattern::Regex(re),
        Err(_) => Pattern::Exact(s.to_string()),
      }
    } else {
      Pattern::Exact(s.to_string())
    }
  }

  fn matches(&self, value: &str) -> bool {
    match self {
      Pattern::Exact(s) => value == s,
      Pattern::Regex(re) => re.is_match(value),
    }
  }
}

fn parse_options(context: &RuleContext) -> HashMap<String, Vec<Pattern>> {
  let secondary = match context.secondary_options() {
    Some(opts) => opts,
    None => return HashMap::new(),
  };

  let properties = match secondary.get("properties") {
    Some(serde_json::Value::Object(obj)) => obj,
    _ => return HashMap::new(),
  };

  let mut map = HashMap::new();
  for (prop, patterns_val) in properties {
    if let Some(arr) = patterns_val.as_array() {
      let patterns: Vec<Pattern> = arr
        .iter()
        .filter_map(|v| v.as_str().map(Pattern::from_str))
        .collect();
      map.insert(prop.to_lowercase(), patterns);
    }
  }
  map
}

/// Extract individual value tokens from a CSS value string.
/// Splits on whitespace and commas, and also extracts arguments from
/// function calls like `calc()`, `var()`, etc.
fn extract_value_tokens(value: &str) -> Vec<&str> {
  let trimmed = value.trim();
  if trimmed.is_empty() {
    return vec![];
  }

  // Split on whitespace and commas, filter empties
  let mut tokens: Vec<&str> = Vec::new();
  let mut start = 0;
  let bytes = trimmed.as_bytes();
  let len = bytes.len();
  let mut paren_depth = 0;
  let mut i = 0;

  while i < len {
    match bytes[i] {
      b'(' => paren_depth += 1,
      b')' => {
        if paren_depth > 0 {
          paren_depth -= 1;
        }
      }
      b' ' | b'\t' | b'\n' | b'\r' | b',' if paren_depth == 0 => {
        let token = trimmed[start..i].trim();
        if !token.is_empty() {
          tokens.push(token);
        }
        start = i + 1;
      }
      _ => {}
    }
    i += 1;
  }

  // Last token
  let token = trimmed[start..].trim();
  if !token.is_empty() {
    tokens.push(token);
  }

  tokens
}

/// Check if a value (possibly containing functions like calc/var) matches
/// any of the allowed patterns. Each top-level token is checked individually.
fn value_matches_patterns(value: &str, patterns: &[Pattern]) -> bool {
  let tokens = extract_value_tokens(value);
  if tokens.is_empty() {
    return true;
  }

  // Every token must match at least one pattern
  for token in &tokens {
    let token_matches = patterns.iter().any(|p| p.matches(token));
    if !token_matches {
      // Also check the whole value in case patterns are meant for the
      // combined value (e.g., shorthand properties).
      return patterns.iter().any(|p| p.matches(value));
    }
  }
  true
}

impl Rule for PluginEnforceVariableForProperty {
  fn name(&self) -> &'static str {
    "plugin/enforce-variable-for-property"
  }

  fn description(&self) -> &'static str {
    "Enforce variable/token usage for specified properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let property_map = parse_options(ctx);
    if property_map.is_empty() {
      return vec![];
    }

    let declarations: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    let mut diags = Vec::new();
    for decl in declarations {
      let prop_lower = decl.property.to_lowercase();
      if let Some(patterns) = property_map.get(&prop_lower) {
        if !value_matches_patterns(&decl.value, patterns) {
          diags.push(
            Diagnostic::new(
              self.name(),
              format!(
                "Expected variable or allowed value for '{}', got '{}'",
                decl.property, decl.value
              ),
            )
            .severity(self.default_severity())
            .span(Span::new(decl.span.offset, decl.span.length)),
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
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};
  use serde_json::json;

  fn ctx_with_options(opts: &serde_json::Value) -> RuleContext<'_> {
    RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(opts),
    }
  }

  fn style_with_decl(prop: &str, val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, 10),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      children: vec![],
      nested_at_rules: vec![],
    })
  }

  #[test]
  fn allows_matching_variable() {
    let rule = PluginEnforceVariableForProperty;
    let opts = json!([true, {
        "properties": {
            "color": ["/^\\$text-/", "inherit", "currentColor", "transparent"]
        }
    }]);
    let ctx = ctx_with_options(&opts);
    let node = style_with_decl("color", "$text-primary");
    let diags = rule.check(&node, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn allows_exact_value() {
    let rule = PluginEnforceVariableForProperty;
    let opts = json!([true, {
        "properties": {
            "color": ["/^\\$text-/", "inherit", "currentColor", "transparent"]
        }
    }]);
    let ctx = ctx_with_options(&opts);
    let node = style_with_decl("color", "inherit");
    let diags = rule.check(&node, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn rejects_disallowed_value() {
    let rule = PluginEnforceVariableForProperty;
    let opts = json!([true, {
        "properties": {
            "color": ["/^\\$text-/", "inherit", "currentColor", "transparent"]
        }
    }]);
    let ctx = ctx_with_options(&opts);
    let node = style_with_decl("color", "red");
    let diags = rule.check(&node, &ctx);
    assert_eq!(diags.len(), 1);
    assert!(
      diags[0]
        .message
        .contains("Expected variable or allowed value")
    );
    assert!(diags[0].message.contains("'color'"));
    assert!(diags[0].message.contains("'red'"));
  }

  #[test]
  fn case_insensitive_property_match() {
    let rule = PluginEnforceVariableForProperty;
    let opts = json!([true, {
        "properties": {
            "Color": ["inherit"]
        }
    }]);
    let ctx = ctx_with_options(&opts);
    let node = style_with_decl("color", "red");
    let diags = rule.check(&node, &ctx);
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn ignores_unconfigured_property() {
    let rule = PluginEnforceVariableForProperty;
    let opts = json!([true, {
        "properties": {
            "color": ["inherit"]
        }
    }]);
    let ctx = ctx_with_options(&opts);
    let node = style_with_decl("font-size", "16px");
    let diags = rule.check(&node, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn no_options_returns_empty() {
    let rule = PluginEnforceVariableForProperty;
    let ctx = RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    let node = style_with_decl("color", "red");
    let diags = rule.check(&node, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn regex_with_case_insensitive_flag() {
    let rule = PluginEnforceVariableForProperty;
    let opts = json!([true, {
        "properties": {
            "color": ["/^inherit$/i"]
        }
    }]);
    let ctx = ctx_with_options(&opts);
    let node = style_with_decl("color", "Inherit");
    let diags = rule.check(&node, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn multiple_properties() {
    let rule = PluginEnforceVariableForProperty;
    let opts = json!([true, {
        "properties": {
            "color": ["inherit"],
            "background-color": ["transparent"]
        }
    }]);
    let ctx = ctx_with_options(&opts);

    let node1 = style_with_decl("color", "red");
    assert_eq!(rule.check(&node1, &ctx).len(), 1);

    let node2 = style_with_decl("background-color", "blue");
    assert_eq!(rule.check(&node2, &ctx).len(), 1);

    let node3 = style_with_decl("background-color", "transparent");
    assert!(rule.check(&node3, &ctx).is_empty());
  }

  #[test]
  fn allows_zero_and_auto_for_margin() {
    let rule = PluginEnforceVariableForProperty;
    let opts = json!([true, {
        "properties": {
            "margin": ["/^\\$spacer/", "0", "auto"]
        }
    }]);
    let ctx = ctx_with_options(&opts);

    let node = style_with_decl("margin", "0");
    assert!(rule.check(&node, &ctx).is_empty());

    let node = style_with_decl("margin", "auto");
    assert!(rule.check(&node, &ctx).is_empty());

    let node = style_with_decl("margin", "$spacer-3");
    assert!(rule.check(&node, &ctx).is_empty());

    let node = style_with_decl("margin", "10px");
    assert_eq!(rule.check(&node, &ctx).len(), 1);
  }

  #[test]
  fn handles_declaration_node_directly() {
    let rule = PluginEnforceVariableForProperty;
    let opts = json!([true, {
        "properties": {
            "color": ["inherit"]
        }
    }]);
    let ctx = ctx_with_options(&opts);
    let node = CssNode::Declaration(Declaration {
      property: "color".to_string(),
      value: "red".to_string(),
      span: ParserSpan::new(0, 10),
      important: false,
    });
    let diags = rule.check(&node, &ctx);
    assert_eq!(diags.len(), 1);
  }
}
