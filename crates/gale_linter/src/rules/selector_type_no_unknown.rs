use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::data::is_known_html_element;
use crate::rule::{Rule, RuleContext};

/// Reports unknown type selectors (HTML element names).
///
/// Custom elements (containing a hyphen like `my-component`) are skipped.
///
/// Equivalent to Stylelint's `selector-type-no-unknown` rule.
pub struct SelectorTypeNoUnknown;

impl Rule for SelectorTypeNoUnknown {
  fn name(&self) -> &'static str {
    "selector-type-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown type selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let selector = if matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass | gale_css_parser::Syntax::Less
    ) {
      strip_scss_line_comments(&rule.selector)
    } else {
      rule.selector.clone()
    };

    let mut diags = Vec::new();
    for name in extract_type_selectors(&selector) {
      // Skip custom elements (contain a hyphen)
      if name.contains('-') {
        continue;
      }
      // Skip keyframe selectors (`from`, `to`).
      if name.eq_ignore_ascii_case("from") || name.eq_ignore_ascii_case("to") {
        continue;
      }
      if !is_known_html_element(&name) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected unknown type selector \"{name}\""),
          )
          .severity(self.default_severity())
          .span(Span::new(rule.span.offset, rule.span.length)),
        );
      }
    }
    diags
  }
}

/// Extract type selectors (bare element names) from a selector string.
///
/// A type selector is a bare identifier at the start of a selector or after
/// a combinator (space, >, +, ~, ,). We skip IDs (#), classes (.),
/// attributes ([), pseudo-classes (:), and pseudo-elements (::).
fn extract_type_selectors(selector: &str) -> Vec<String> {
  let mut types = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  // Whether we expect a type selector at the current position
  let mut expect_type = true;

  while i < len {
    let ch = chars[i];

    match ch {
      // Skip block comments (/* ... */)
      '/' if i + 1 < len && chars[i + 1] == '*' => {
        i += 2;
        while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
          i += 1;
        }
        if i + 1 < len {
          i += 2; // skip closing */
        }
        expect_type = true;
        continue;
      }
      // Skip pseudo-elements (::name)
      ':' if i + 1 < len && chars[i + 1] == ':' => {
        i += 2;
        while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
          i += 1;
        }
        expect_type = false;
        continue;
      }
      // Skip pseudo-classes (:name) and their arguments
      ':' => {
        i += 1;
        while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
          i += 1;
        }
        if i < len && chars[i] == '(' {
          let mut depth = 1;
          i += 1;
          while i < len && depth > 0 {
            if chars[i] == '(' {
              depth += 1;
            } else if chars[i] == ')' {
              depth -= 1;
            }
            i += 1;
          }
        }
        expect_type = false;
        continue;
      }
      // Skip class selectors (.name)
      '.' => {
        i += 1;
        while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
          i += 1;
        }
        expect_type = false;
        continue;
      }
      // Skip ID selectors (#name)
      '#' => {
        i += 1;
        while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
          i += 1;
        }
        expect_type = false;
        continue;
      }
      // Skip attribute selectors ([...])
      '[' => {
        let mut depth = 1;
        i += 1;
        while i < len && depth > 0 {
          if chars[i] == '[' {
            depth += 1;
          } else if chars[i] == ']' {
            depth -= 1;
          }
          i += 1;
        }
        expect_type = false;
        continue;
      }
      // Combinators reset to expecting a type selector
      '>' | '+' | '~' | ',' => {
        i += 1;
        expect_type = true;
        continue;
      }
      // Universal selector
      '*' => {
        i += 1;
        expect_type = false;
        continue;
      }
      // The & nesting selector
      '&' => {
        i += 1;
        expect_type = false;
        continue;
      }
      // Whitespace acts as descendant combinator
      c if c.is_ascii_whitespace() => {
        i += 1;
        expect_type = true;
        continue;
      }
      // Identifier character — could be a type selector
      c if c.is_ascii_alphabetic() && expect_type => {
        let start = i;
        while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
          i += 1;
        }
        let name: String = chars[start..i].iter().collect();
        types.push(name);
        expect_type = false;
        continue;
      }
      _ => {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
          // Skip over identifiers we don't care about
          while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_')
          {
            i += 1;
          }
        } else {
          i += 1;
        }
        expect_type = false;
        continue;
      }
    }
  }

  types
}

/// Strip `//` line comments from a selector string (SCSS/Less).
/// Handles both full-line comments and inline trailing comments.
fn strip_scss_line_comments(selector: &str) -> String {
  selector
    .lines()
    .map(|line| {
      // Find `//` that's not inside a string
      let mut in_single = false;
      let mut in_double = false;
      let bytes = line.as_bytes();
      let mut i = 0;
      while i < bytes.len() {
        match bytes[i] {
          b'\'' if !in_double => in_single = !in_single,
          b'"' if !in_single => in_double = !in_double,
          b'/' if !in_single && !in_double && i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
            return &line[..i];
          }
          _ => {}
        }
        i += 1;
      }
      line
    })
    .collect::<Vec<_>>()
    .join("\n")
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

  fn style(sel: &str) -> CssNode {
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
  fn reports_unknown_type_selector() {
    let d = SelectorTypeNoUnknown.check(&style("fakeelement"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("fakeelement"));
  }

  #[test]
  fn allows_known_html_elements() {
    assert!(
      SelectorTypeNoUnknown
        .check(&style("div"), &ctx())
        .is_empty()
    );
    assert!(SelectorTypeNoUnknown.check(&style("a"), &ctx()).is_empty());
    assert!(
      SelectorTypeNoUnknown
        .check(&style("span"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn skips_custom_elements() {
    assert!(
      SelectorTypeNoUnknown
        .check(&style("my-component"), &ctx())
        .is_empty()
    );
    assert!(
      SelectorTypeNoUnknown
        .check(&style("app-header"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn ignores_class_and_id_selectors() {
    assert!(
      SelectorTypeNoUnknown
        .check(&style(".foo"), &ctx())
        .is_empty()
    );
    assert!(
      SelectorTypeNoUnknown
        .check(&style("#bar"), &ctx())
        .is_empty()
    );
  }
}
