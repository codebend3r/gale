use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Limit the number of ID selectors in a selector.
///
/// Equivalent to Stylelint's `selector-max-id` rule.
/// Default maximum: 0 (disallow ID selectors entirely).
pub struct SelectorMaxId;

const MAX_ID: usize = 0;

impl Rule for SelectorMaxId {
  fn name(&self) -> &'static str {
    "selector-max-id"
  }

  fn description(&self) -> &'static str {
    "Limit the number of ID selectors in a selector"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    // Read configured max from options (primary option is a number).
    // Options may be a plain number or an array [number, { secondary }].
    let max = ctx
      .primary_option()
      .and_then(|v| v.as_u64())
      .map(|n| n as usize)
      .unwrap_or(MAX_ID);

    // Stylelint checks each individual selector in a comma-separated
    // selector list and reports a separate diagnostic for each one that
    // exceeds the limit.  Match this behaviour.
    let mut diags = Vec::new();

    // Find the byte offset of each individual selector within the source
    // so we can report accurate positions.
    let selector_source_start = rule.span.offset;

    // Read the original selector text from the source to preserve exact
    // formatting (pseudo-element notation, attribute quoting, etc.).
    // We find the opening `{` to determine where the selector ends,
    // since the parser may normalize whitespace differently from source.
    let source_selector = {
      if selector_source_start < ctx.source.len() {
        let rest = &ctx.source[selector_source_start..];
        // Find the opening brace, respecting strings and brackets.
        let end = find_selector_end(rest);
        rest[..end].trim_end()
      } else {
        &rule.selector[..]
      }
    };

    let parts = split_selector_list(source_selector);
    // Calculate byte offsets of each part within the selector string.
    let mut part_offsets: Vec<usize> = Vec::new();
    {
      let mut pos = 0;
      for (idx, part) in parts.iter().enumerate() {
        // After the first part, skip past the comma separator.
        if idx > 0 {
          pos += 1; // the comma itself
        }
        // Find where this part starts (skip leading whitespace in the source selector)
        let selector_bytes = source_selector.as_bytes();
        while pos < selector_bytes.len()
          && (selector_bytes[pos] == b' '
            || selector_bytes[pos] == b'\t'
            || selector_bytes[pos] == b'\n'
            || selector_bytes[pos] == b'\r')
        {
          pos += 1;
        }
        part_offsets.push(pos);
        pos += part.trim_start().len();
      }
    }

    for (idx, individual) in parts.iter().enumerate() {
      let trimmed = individual.trim();
      if trimmed.is_empty() {
        continue;
      }
      let count = count_id_selectors(trimmed);
      if count > max {
        // Use the offset of this individual selector within the
        // source for accurate line/column reporting.
        let offset = selector_source_start + part_offsets.get(idx).copied().unwrap_or(0);
        diags.push(
          Diagnostic::new(
            self.name(),
            format!(
              "Expected \"{}\" to have no more than {max} ID selector{}",
              trimmed,
              if max == 1 { "" } else { "s" },
            ),
          )
          .severity(self.default_severity())
          .span(Span::new(offset, trimmed.len())),
        );
      }
    }
    diags
  }
}

/// Find the end of the selector text in source by locating the opening `{`.
/// This accounts for quoted strings and attribute selectors containing `{`.
fn find_selector_end(source: &str) -> usize {
  let bytes = source.as_bytes();
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'{' => return i,
      b'"' | b'\'' => {
        // Skip quoted string.
        let quote = bytes[i];
        i += 1;
        while i < bytes.len() && bytes[i] != quote {
          if bytes[i] == b'\\' {
            i += 1; // skip escaped char
          }
          i += 1;
        }
      }
      b'[' => {
        // Skip attribute selector.
        i += 1;
        while i < bytes.len() && bytes[i] != b']' {
          if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
              if bytes[i] == b'\\' {
                i += 1;
              }
              i += 1;
            }
          }
          i += 1;
        }
      }
      _ => {}
    }
    i += 1;
  }
  source.len()
}

/// Split a selector list by commas, respecting parentheses and brackets.
/// E.g. `"#a .b, #c .d"` → `["#a .b", "#c .d"]`
fn split_selector_list(selector: &str) -> Vec<&str> {
  let mut parts = Vec::new();
  let mut depth = 0usize;
  let mut start = 0;
  let bytes = selector.as_bytes();

  for (i, &b) in bytes.iter().enumerate() {
    match b {
      b'(' | b'[' => depth += 1,
      b')' | b']' => depth = depth.saturating_sub(1),
      b',' if depth == 0 => {
        parts.push(&selector[start..i]);
        start = i + 1;
      }
      _ => {}
    }
  }
  parts.push(&selector[start..]);
  parts
}

/// Count `#` characters that are ID selectors (not hex color fragments or SCSS interpolation).
/// A `#` is an ID selector if it is followed by a CSS identifier start character
/// (letter, underscore, hyphen, or non-ASCII) but NOT `{` (which indicates SCSS interpolation `#{...}`).
fn count_id_selectors(selector: &str) -> usize {
  let chars: Vec<char> = selector.chars().collect();
  let mut count = 0;
  let mut i = 0;
  while i < chars.len() {
    if chars[i] == '#'
      && let Some(&next) = chars.get(i + 1)
    {
      if next == '{' {
        // SCSS interpolation #{...} — skip past the closing `}`
        i += 2;
        let mut depth = 1;
        while i < chars.len() && depth > 0 {
          if chars[i] == '{' {
            depth += 1;
          } else if chars[i] == '}' {
            depth -= 1;
          }
          i += 1;
        }
        continue;
      } else if next.is_ascii_alphabetic() || next == '_' || next == '-' || !next.is_ascii() {
        count += 1;
      }
    }
    i += 1;
  }
  count
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
  fn reports_id_selector() {
    let d = SelectorMaxId.check(&style_with_selector("#foo"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#foo"));
  }

  #[test]
  fn allows_class_selector() {
    let d = SelectorMaxId.check(&style_with_selector(".bar"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn counts_multiple_ids() {
    let d = SelectorMaxId.check(&style_with_selector("#a #b"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#a #b"));
  }

  #[test]
  fn skips_scss_interpolation() {
    // #{$var} is SCSS interpolation, not an ID selector
    let d = SelectorMaxId.check(&style_with_selector(".#{$prefix}-item"), &ctx());
    assert!(
      d.is_empty(),
      "SCSS interpolation should not count as ID selector"
    );
  }
}
