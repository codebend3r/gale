use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require a newline or disallow whitespace after the commas of selector lists.
///
/// Equivalent to `@stylistic/selector-list-comma-newline-after`.
pub struct StylisticSelectorListCommaNewlineAfter;

impl Rule for StylisticSelectorListCommaNewlineAfter {
  fn name(&self) -> &'static str {
    "@stylistic/selector-list-comma-newline-after"
  }

  fn description(&self) -> &'static str {
    "Require a newline or disallow whitespace after the commas of selector lists"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let option = ctx.primary_option_str().unwrap_or("always-multi-line");
    let selector = &rule.selector;

    // Check if this is a selector list (contains commas)
    if !selector.contains(',') {
      return vec![];
    }

    let is_multi_line = selector.contains('\n');

    let mut diags = Vec::new();
    let sel_offset = rule.span.offset;

    // Find commas in the source text of the selector for accurate spans
    let sel_source = if sel_offset + selector.len() <= ctx.source.len() {
      &ctx.source[sel_offset..sel_offset + selector.len()]
    } else {
      selector.as_str()
    };

    let bytes = sel_source.as_bytes();
    let len = bytes.len();
    let mut paren_depth = 0i32;
    let mut in_interpolation = 0i32;
    let mut i = 0;
    while i < len {
      let b = bytes[i];

      // Skip SCSS line comments entirely (from `//` to end of line)
      if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }

      // Track SCSS interpolation #{...}
      if b == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
        in_interpolation += 1;
      }
      if b == b'{' && i > 0 && bytes[i - 1] != b'#' {
        // regular brace, not interpolation
      } else if b == b'{' && i > 0 && bytes[i - 1] == b'#' {
        // already counted above
      }
      if b == b'}' && in_interpolation > 0 {
        in_interpolation -= 1;
      }
      if b == b'(' {
        paren_depth += 1;
      }
      if b == b')' && paren_depth > 0 {
        paren_depth -= 1;
      }
      // Only check commas at the top level of the selector
      // (not inside interpolation or function calls)
      if b == b',' && paren_depth == 0 && in_interpolation == 0 {
        // Stylelint (PostCSS) can't parse selectors where a
        // comma-separated part contains SCSS interpolation.  Skip
        // commas that are immediately followed (after optional
        // whitespace) by `#{` to avoid false positives.
        {
          let mut skip = i + 1;
          while skip < len && (bytes[skip] == b' ' || bytes[skip] == b'\t') {
            skip += 1;
          }
          if skip + 1 < len && bytes[skip] == b'#' && bytes[skip + 1] == b'{' {
            i += 1;
            continue;
          }
        }
        let after_comma = i + 1;
        let next_char = if after_comma < bytes.len() {
          Some(bytes[after_comma] as char)
        } else {
          None
        };

        // Check for newline after comma, treating comments as transparent.
        // After optional whitespace, if we find:
        //   - `//` (SCSS line comment): treat as newline (comment runs to EOL)
        //   - `/* ... */` followed by newline: treat as newline
        //   - `\n` or `\r\n`: normal newline
        let has_newline_after = {
          let mut skip = after_comma;
          while skip < bytes.len() && (bytes[skip] == b' ' || bytes[skip] == b'\t') {
            skip += 1;
          }
          if skip + 1 < bytes.len() && bytes[skip] == b'/' && bytes[skip + 1] == b'/' {
            // SCSS line comment — implicitly ends with newline
            true
          } else if skip + 1 < bytes.len() && bytes[skip] == b'/' && bytes[skip + 1] == b'*' {
            // Block comment — skip to end, then check for newline
            let mut j = skip + 2;
            while j + 1 < bytes.len() && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
              j += 1;
            }
            if j + 1 < bytes.len() {
              j += 2; // skip past `*/`
            }
            // Skip whitespace after closing `*/`
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
              j += 1;
            }
            j < bytes.len() && (bytes[j] == b'\n' || bytes[j] == b'\r')
          } else {
            next_char == Some('\n')
              || (next_char == Some('\r')
                && after_comma + 1 < bytes.len()
                && bytes[after_comma + 1] == b'\n')
          }
        };

        let violation = match option {
          "always" => !has_newline_after,
          "never" => next_char.is_some_and(|c| c == '\n' || c == '\r'),
          "always-multi-line" => is_multi_line && !has_newline_after,
          _ => false,
        };

        if violation {
          let msg = match option {
            "always" | "always-multi-line" => "Expected newline after \",\"",
            "never" => "Unexpected newline after \",\"",
            _ => continue,
          };
          diags.push(
            Diagnostic::new(self.name(), msg)
              .severity(self.default_severity())
              .span(Span::new(sel_offset + i, 1)),
          );
        }
      }
      i += 1;
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Span as ParserSpan, StyleRule, Syntax};

  fn ctx_with_source(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_selector(sel: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: vec![],
      span: ParserSpan::new(0, sel.len()),
      ..Default::default()
    })
  }

  #[test]
  fn reports_missing_newline_after_comma_multiline() {
    let rule = StylisticSelectorListCommaNewlineAfter;
    let sel = "a,\nb, c";
    let ctx = ctx_with_source(sel);
    let d = rule.check(&style_with_selector(sel), &ctx);
    // The second comma (b, c) has a space instead of newline
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("newline"));
  }

  #[test]
  fn allows_newline_after_all_commas() {
    let rule = StylisticSelectorListCommaNewlineAfter;
    let sel = "a,\nb,\nc";
    let ctx = ctx_with_source(sel);
    let d = rule.check(&style_with_selector(sel), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_scss_line_comment_after_comma() {
    let rule = StylisticSelectorListCommaNewlineAfter;
    let sel = "&[aria-disabled=\"true\"]:enabled, // This catches a situation\n&[aria-disabled=\"true\"]:active:enabled";
    let ctx = ctx_with_source(sel);
    let d = rule.check(&style_with_selector(sel), &ctx);
    assert!(
      d.is_empty(),
      "Should not flag comma followed by SCSS line comment, got {} diagnostics",
      d.len()
    );
  }

  #[test]
  fn allows_block_comment_newline_after_comma() {
    let rule = StylisticSelectorListCommaNewlineAfter;
    let sel = "a, /* comment */\nb,\nc";
    let ctx = ctx_with_source(sel);
    let d = rule.check(&style_with_selector(sel), &ctx);
    assert!(
      d.is_empty(),
      "Should not flag comma followed by block comment then newline, got {} diagnostics",
      d.len()
    );
  }

  #[test]
  fn allows_single_line_with_default() {
    let rule = StylisticSelectorListCommaNewlineAfter;
    let sel = "a, b, c";
    let ctx = ctx_with_source(sel);
    let d = rule.check(&style_with_selector(sel), &ctx);
    // Default is "always-multi-line", single line is fine
    assert!(d.is_empty());
  }
}
