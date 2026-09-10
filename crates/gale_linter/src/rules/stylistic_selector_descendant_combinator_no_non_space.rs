use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow non-space characters for descendant combinators.
///
/// Descendant combinators should be a single space, not tabs or multiple spaces.
pub struct StylisticSelectorDescendantCombinatorNoNonSpace;

impl Rule for StylisticSelectorDescendantCombinatorNoNonSpace {
  fn name(&self) -> &'static str {
    "@stylistic/selector-descendant-combinator-no-non-space"
  }

  fn description(&self) -> &'static str {
    "Disallow non-space characters for descendant combinators"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let selector = &rule.selector;
    let bytes = selector.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;

    while i < len {
      // Skip SCSS // line comments and trailing newline + indentation
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        // Skip newline + indentation after comment
        while i < len
          && (bytes[i] == b'\n' || bytes[i] == b'\r' || bytes[i] == b' ' || bytes[i] == b'\t')
        {
          i += 1;
        }
        continue;
      }
      // Skip block comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        if i + 1 < len {
          i += 2;
        }
        continue;
      }
      // Skip strings
      if bytes[i] == b'\'' || bytes[i] == b'"' {
        let quote = bytes[i];
        i += 1;
        while i < len && bytes[i] != quote {
          if bytes[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
        if i < len {
          i += 1;
        }
        continue;
      }
      // Skip attribute selectors entirely
      if bytes[i] == b'[' {
        i += 1;
        while i < len && bytes[i] != b']' {
          if bytes[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
        if i < len {
          i += 1;
        }
        continue;
      }

      // Skip newlines (descendant combinators are same-line whitespace)
      if bytes[i] == b'\n' || bytes[i] == b'\r' {
        i += 1;
        continue;
      }

      // Skip commas (selector list separators — whitespace after them is formatting)
      if bytes[i] == b',' {
        i += 1;
        // Skip all whitespace after comma (including newlines + indentation)
        while i < len
          && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\n' || bytes[i] == b'\r')
        {
          i += 1;
        }
        continue;
      }

      // Check for whitespace gaps that could be descendant combinators
      if bytes[i] == b' ' || bytes[i] == b'\t' {
        let ws_start = i;
        // Collect all whitespace (on same line for descendant combinator)
        while i < len && (bytes[i] == b' ' || bytes[i] == b'\t') {
          i += 1;
        }
        // If we hit end, a newline, or a combinator operator, skip
        if i >= len {
          continue;
        }
        // Newline after whitespace — this is multi-line selector formatting, not a combinator
        if bytes[i] == b'\n' || bytes[i] == b'\r' {
          continue;
        }
        // Check the character after whitespace: if it's a combinator
        // operator (>, +, ~) this is NOT a descendant combinator
        if bytes[i] == b'>' || bytes[i] == b'+' || bytes[i] == b'~' {
          i += 1;
          continue;
        }
        // Also check the character before the whitespace: if it was
        // a combinator operator, this is not a descendant combinator
        if ws_start > 0
          && (bytes[ws_start - 1] == b'>'
            || bytes[ws_start - 1] == b'+'
            || bytes[ws_start - 1] == b'~')
        {
          continue;
        }
        // If it's a comma or newline, this is a selector list separator, not a combinator
        if bytes[i] == b',' || bytes[i] == b'\n' || bytes[i] == b'\r' {
          continue;
        }
        // Also skip if whitespace is at the start (before first selector)
        if ws_start == 0 {
          continue;
        }
        // Also skip if the character before whitespace is a comma or opening brace
        if bytes[ws_start - 1] == b',' || bytes[ws_start - 1] == b'{' {
          continue;
        }

        let ws_len = i - ws_start;
        let ws_slice = &selector[ws_start..i];

        // Flag if the whitespace contains a tab or is more than one space
        if ws_slice.contains('\t') || ws_len > 1 {
          diagnostics.push(
            Diagnostic::new(
              self.name(),
              "Expected single space for descendant combinator",
            )
            .severity(self.default_severity())
            .span(Span::new(rule.span.offset + ws_start, ws_len)),
          );
        }
        continue;
      }

      i += 1;
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn check_selector(selector: &str) -> Vec<Diagnostic> {
    let rule = StylisticSelectorDescendantCombinatorNoNonSpace;
    let ctx = RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    let node = CssNode::Style(StyleRule {
      selector: selector.to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    });
    rule.check(&node, &ctx)
  }

  #[test]
  fn allows_single_space_descendant_combinator() {
    let d = check_selector("a b");
    assert!(d.is_empty());
  }

  #[test]
  fn rejects_tab_descendant_combinator() {
    let d = check_selector("a\tb");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("single space"));
  }

  #[test]
  fn rejects_multiple_spaces_descendant_combinator() {
    let d = check_selector("a  b");
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_single_space_around_child_combinator() {
    let d = check_selector("a > b");
    assert!(d.is_empty());
  }

  #[test]
  fn allows_single_space_around_adjacent_combinator() {
    let d = check_selector("a + b");
    assert!(d.is_empty());
  }
}
