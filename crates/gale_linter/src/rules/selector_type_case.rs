use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require lowercase or uppercase for type selectors.
///
/// In "lower" mode (default), flags uppercase type selectors like `DIV`, `SPAN`, etc.
/// Ignores pseudo-elements, pseudo-classes, class selectors, ID selectors,
/// attribute selectors, and case-sensitive SVG elements.
///
/// Equivalent to Stylelint's `selector-type-case` rule with "lower" option.
pub struct SelectorTypeCase;

/// SVG elements that are case-sensitive and should be skipped.
const SVG_CASE_SENSITIVE: &[&str] = &[
  "altGlyph",
  "altGlyphDef",
  "altGlyphItem",
  "animateColor",
  "animateMotion",
  "animateTransform",
  "clipPath",
  "feBlend",
  "feColorMatrix",
  "feComponentTransfer",
  "feComposite",
  "feConvolveMatrix",
  "feDiffuseLighting",
  "feDisplacementMap",
  "feDistantLight",
  "feDropShadow",
  "feFlood",
  "feFuncA",
  "feFuncB",
  "feFuncG",
  "feFuncR",
  "feGaussianBlur",
  "feImage",
  "feMerge",
  "feMergeNode",
  "feMorphology",
  "feOffset",
  "fePointLight",
  "feSpecularLighting",
  "feSpotLight",
  "feTile",
  "feTurbulence",
  "foreignObject",
  "glyphRef",
  "linearGradient",
  "radialGradient",
  "textPath",
];

impl Rule for SelectorTypeCase {
  fn name(&self) -> &'static str {
    "selector-type-case"
  }

  fn description(&self) -> &'static str {
    "Specify lowercase or uppercase for type selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let mut diags = Vec::new();

    for type_sel in extract_type_selectors(&rule.selector) {
      // Skip SVG case-sensitive elements.
      if SVG_CASE_SENSITIVE.contains(&type_sel.as_str()) {
        continue;
      }
      // In "lower" mode: flag if any character is uppercase.
      if type_sel.chars().any(|c| c.is_ascii_uppercase()) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!(
              "Expected \"{}\" to be \"{}\"",
              type_sel,
              type_sel.to_ascii_lowercase()
            ),
          )
          .severity(self.default_severity())
          .span(Span::new(rule.span.offset, rule.span.length)),
        );
      }
    }

    diags
  }
}

/// Extract type selectors from a CSS selector string.
///
/// Type selectors are bare element names (e.g., `div`, `span`, `a`).
/// This skips class selectors (`.foo`), ID selectors (`#bar`), attribute
/// selectors (`[attr]`), pseudo-classes (`:hover`), pseudo-elements (`::before`),
/// and combinators (`>`, `+`, `~`).
fn extract_type_selectors(selector: &str) -> Vec<String> {
  let mut results = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    let ch = chars[i];

    // Skip SCSS/Less interpolation blocks: #{...} or @{...}
    // Also consume any trailing identifier characters that are part of the
    // interpolated name (e.g. `.#{$prefix}__itemsWrapper` — the suffix
    // `__itemsWrapper` belongs to the same class selector).
    if (ch == '#' || ch == '@') && i + 1 < len && chars[i + 1] == '{' {
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
      // Consume trailing identifier chars (part of the interpolated token)
      while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    // Skip SCSS variables ($var)
    if ch == '$' {
      i += 1;
      while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    // Skip SCSS line comments (// ...)
    if ch == '/' && i + 1 < len && chars[i + 1] == '/' {
      while i < len && chars[i] != '\n' {
        i += 1;
      }
      continue;
    }

    // Skip attribute selectors entirely.
    if ch == '[' {
      while i < len && chars[i] != ']' {
        i += 1;
      }
      i += 1; // skip ']'
      continue;
    }

    // Skip class selectors.
    if ch == '.' {
      i += 1;
      while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    // Skip ID selectors.
    if ch == '#' {
      i += 1;
      while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    // Skip pseudo-elements (::) and pseudo-classes (:).
    if ch == ':' {
      i += 1;
      if i < len && chars[i] == ':' {
        i += 1; // skip second ':'
      }
      // Skip the pseudo name.
      while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      // Skip parenthesized arguments like :nth-child(2n).
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
      continue;
    }

    // Skip combinators and whitespace.
    if ch == '>' || ch == '+' || ch == '~' || ch == ',' || ch.is_whitespace() {
      i += 1;
      continue;
    }

    // Skip the universal selector.
    if ch == '*' {
      i += 1;
      continue;
    }

    // Skip `&` (nesting selector) and any following identifier suffix
    // (e.g. `&_suggestionsWrapper`, `&--modifier`).
    if ch == '&' {
      i += 1;
      while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    // Skip block comments (/* ... */).
    if ch == '/' && i + 1 < len && chars[i + 1] == '*' {
      i += 2;
      while i + 1 < len {
        if chars[i] == '*' && chars[i + 1] == '/' {
          i += 2;
          break;
        }
        i += 1;
      }
      continue;
    }

    // Collect an identifier — this should be a type selector.
    if ch.is_alphabetic() || ch == '_' {
      let start = i;
      while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      results.push(chars[start..i].iter().collect::<String>());
      continue;
    }

    // Skip SCSS placeholder selectors (%placeholder) and keyframe `%`.
    if ch == '%' {
      i += 1;
      while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    // Skip anything else.
    i += 1;
  }

  results
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
  fn reports_uppercase_type_selector() {
    let d = SelectorTypeCase.check(&style_with_selector("DIV"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("div"));
    assert!(d[0].message.contains("DIV"));
  }

  #[test]
  fn allows_lowercase_type_selector() {
    let d = SelectorTypeCase.check(&style_with_selector("div"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_mixed_case_type_selector() {
    let d = SelectorTypeCase.check(&style_with_selector("Div"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("div"));
  }

  #[test]
  fn ignores_class_selector() {
    let d = SelectorTypeCase.check(&style_with_selector(".MyClass"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_id_selector() {
    let d = SelectorTypeCase.check(&style_with_selector("#MyId"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_pseudo_class() {
    let d = SelectorTypeCase.check(&style_with_selector("a:Hover"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_svg_case_sensitive_elements() {
    let d = SelectorTypeCase.check(&style_with_selector("clipPath"), &ctx());
    assert!(d.is_empty());
    let d = SelectorTypeCase.check(&style_with_selector("textPath"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_uppercase_among_complex_selector() {
    let d = SelectorTypeCase.check(&style_with_selector("SPAN.foo > A"), &ctx());
    assert_eq!(d.len(), 2); // SPAN and A
  }

  #[test]
  fn allows_lowercase_complex_selector() {
    let d = SelectorTypeCase.check(&style_with_selector("span.foo > a"), &ctx());
    assert!(d.is_empty());
  }
}
