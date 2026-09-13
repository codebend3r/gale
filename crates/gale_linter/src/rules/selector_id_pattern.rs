use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

const DEFAULT_PATTERN: &str = "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$";

/// Enforces a naming pattern for ID selectors.
///
/// Extracts `#id` patterns from selectors (excluding hex colors in values)
/// and validates them against the configured regex pattern.
///
/// Accepts a regex string as the primary option.
/// Defaults to kebab-case pattern if no option is provided.
///
/// Equivalent to Stylelint's `selector-id-pattern` rule.
pub struct SelectorIdPattern;

impl Rule for SelectorIdPattern {
  fn name(&self) -> &'static str {
    "selector-id-pattern"
  }

  fn description(&self) -> &'static str {
    "Specify a pattern for ID selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let pattern_str = ctx
      .options
      .and_then(|v| v.as_str())
      .unwrap_or(DEFAULT_PATTERN);

    let re = match Regex::new(pattern_str) {
      Ok(r) => r,
      Err(_) => return vec![],
    };

    let mut diags = Vec::new();
    for id in extract_id_selectors(&rule.selector) {
      // Skip IDs containing SCSS interpolation #{...}
      if matches!(
        ctx.syntax,
        gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass
      ) && id.contains("#{")
      {
        continue;
      }
      if !re.is_match(&id) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Expected ID selector \"#{id}\" to match pattern \"{pattern_str}\""),
          )
          .severity(self.default_severity())
          .span(Span::new(rule.span.offset, rule.span.length)),
        );
      }
    }
    diags
  }
}

/// Extract ID names from a selector string.
/// Finds `#name` patterns that are valid CSS ID selectors — skipping `#`
/// followed by only hex digits (which would be ambiguous with hex colors,
/// though in selectors this is less common).
fn extract_id_selectors(selector: &str) -> Vec<String> {
  let mut ids = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    if chars[i] == '#' {
      // Skip SCSS interpolation #{...}
      if i + 1 < len && chars[i + 1] == '{' {
        i += 2;
        let mut depth = 1;
        while i < len && depth > 0 {
          if chars[i] == '{' {
            depth += 1;
          } else if chars[i] == '}' {
            depth -= 1;
          }
          i += 1;
        }
        continue;
      }

      i += 1;
      let start = i;
      // CSS ident chars: alphanum, hyphen, underscore, non-ASCII
      while i < len
        && (chars[i].is_ascii_alphanumeric()
          || chars[i] == '-'
          || chars[i] == '_'
          || !chars[i].is_ascii())
      {
        i += 1;
      }
      if i > start {
        let name: String = chars[start..i].iter().collect();
        // Skip if name looks purely like a hex color (all hex digits, 3/4/6/8 chars)
        let is_hex = name.chars().all(|c| c.is_ascii_hexdigit());
        let hex_len = name.len();
        if !(is_hex && (hex_len == 3 || hex_len == 4 || hex_len == 6 || hex_len == 8)) {
          ids.push(name);
        }
      }
    } else {
      i += 1;
    }
  }

  ids
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn ctx_with_options(opts: &serde_json::Value) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(opts),
    }
  }

  fn style_with_selector(sel: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_camel_case_id() {
    let d = SelectorIdPattern.check(&style_with_selector("#myId"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myId"));
  }

  #[test]
  fn allows_kebab_case_id() {
    assert!(
      SelectorIdPattern
        .check(&style_with_selector("#my-id"), &ctx())
        .is_empty()
    );
    assert!(
      SelectorIdPattern
        .check(&style_with_selector("#foo"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_underscore_id() {
    let d = SelectorIdPattern.check(&style_with_selector("#my_id"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_multiple_bad_ids() {
    let d = SelectorIdPattern.check(&style_with_selector("#myId #anotherId"), &ctx());
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn custom_pattern_camel_case() {
    let opts = serde_json::json!("^[a-z][a-zA-Z0-9]+$");
    let c = ctx_with_options(&opts);
    // camelCase should pass
    assert!(
      SelectorIdPattern
        .check(&style_with_selector("#myId"), &c)
        .is_empty()
    );
    // kebab-case should fail
    let d = SelectorIdPattern.check(&style_with_selector("#my-id"), &c);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn ignores_non_style_nodes() {
    let node = CssNode::AtRule(gale_css_parser::AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    });
    let d = SelectorIdPattern.check(&node, &ctx());
    assert!(d.is_empty());
  }
}
