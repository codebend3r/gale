use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Specify indentation.
///
/// Equivalent to `@stylistic/indentation`.
pub struct StylisticIndentation;

impl Rule for StylisticIndentation {
  fn name(&self) -> &'static str {
    "@stylistic/indentation"
  }

  fn description(&self) -> &'static str {
    "Specify indentation"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let primary = context.primary_option();
    let use_tab: bool;
    let indent_size: usize;

    match primary {
      Some(serde_json::Value::String(s)) if s == "tab" => {
        use_tab = true;
        indent_size = 1;
      }
      Some(serde_json::Value::Number(n)) => {
        use_tab = false;
        indent_size = n.as_u64().unwrap_or(2) as usize;
      }
      _ => {
        use_tab = false;
        indent_size = 2;
      }
    }

    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;
    let mut brace_depth: i32 = 0;
    let mut paren_depth: i32 = 0;
    let mut line_start = 0;
    // Track the last meaningful (non-whitespace, non-comment) character
    // to detect multi-line value continuation lines.
    let mut last_meaningful_char: u8 = b';'; // start as if after a statement
    // Track how many "value indent levels" deep we are. This increments
    // when we see a newline after an opening paren `(`, and decrements
    // when we see `)` as the first meaningful char on a new line.
    let mut value_indent_depth: i32 = 0;
    // Whether the current multi-line value started on the same line as
    // the property colon. Only these contexts get indentation checks.
    // E.g. `color: calc(\n var(...)` → true, `color:\n var(...)` → false.
    let mut value_started_on_colon_line = false;
    // Set to true when we see `:` in a declaration context, cleared on newline
    let mut colon_seen_this_line = false;
    // Set to true when we see `@include` on this line (SCSS mixin calls)
    let mut at_include_seen_this_line = false;

    while i < len {
      // Skip strings
      if bytes[i] == b'"' || bytes[i] == b'\'' {
        let quote = bytes[i];
        last_meaningful_char = quote;
        i += 1;
        while i < len && bytes[i] != quote {
          if bytes[i] == b'\\' {
            i += 1;
          }
          if i < len && bytes[i] == b'\n' {
            line_start = i + 1;
          }
          i += 1;
        }
        if i < len {
          last_meaningful_char = bytes[i]; // closing quote
          i += 1;
        }
        continue;
      }

      // Skip block comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          if bytes[i] == b'\n' {
            line_start = i + 1;
          }
          i += 1;
        }
        if i + 1 < len {
          i += 2;
        }
        // Don't update last_meaningful_char — comments don't count
        continue;
      }

      // Skip SCSS line comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        // Don't update last_meaningful_char — comments don't count
        continue;
      }

      // Skip SCSS interpolation #{...}
      if bytes[i] == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
        i += 2;
        let mut interp_depth = 1;
        while i < len && interp_depth > 0 {
          if bytes[i] == b'{' {
            interp_depth += 1;
          } else if bytes[i] == b'}' {
            interp_depth -= 1;
          }
          if interp_depth > 0 {
            i += 1;
          }
        }
        if i < len {
          i += 1;
        }
        last_meaningful_char = b'}';
        continue;
      }

      if bytes[i] == b'{' {
        brace_depth += 1;
        last_meaningful_char = b'{';
        value_indent_depth = 0;
        value_started_on_colon_line = false;
        i += 1;
        continue;
      }

      if bytes[i] == b'}' {
        brace_depth -= 1;
        if brace_depth < 0 {
          brace_depth = 0;
        }
        last_meaningful_char = b'}';
        value_indent_depth = 0;
        value_started_on_colon_line = false;
        i += 1;
        continue;
      }

      // Track parenthesis depth (SCSS maps, function arguments)
      if bytes[i] == b'(' {
        paren_depth += 1;
        // If this is the first `(` in a declaration that started with
        // a colon on the same line, OR this `(` is part of an @include
        // call, mark this value as "checked" for multi-line indentation.
        // Exclude pseudo-class functions like :where(), :not(), etc.
        if paren_depth == 1 && !value_started_on_colon_line {
          let mut is_pseudo = false;
          if i > 0 {
            let mut p = i - 1;
            while p > 0
              && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'-' || bytes[p] == b'_')
            {
              p -= 1;
            }
            if bytes[p] == b':' {
              is_pseudo = true;
            }
          }
          if !is_pseudo && (colon_seen_this_line || at_include_seen_this_line) {
            value_started_on_colon_line = true;
          }
        }
        last_meaningful_char = b'(';
        i += 1;
        continue;
      }
      if bytes[i] == b')' {
        paren_depth -= 1;
        if paren_depth < 0 {
          paren_depth = 0;
        }
        if paren_depth == 0 {
          value_started_on_colon_line = false;
        }
        last_meaningful_char = b')';
        i += 1;
        continue;
      }

      if bytes[i] == b';' {
        last_meaningful_char = b';';
        value_indent_depth = 0;
        value_started_on_colon_line = false;
        i += 1;
        continue;
      }

      if bytes[i] == b'\n' {
        line_start = i + 1;
        colon_seen_this_line = false;
        at_include_seen_this_line = false;

        // If the last meaningful char before this newline was `(`,
        // it means a paren was opened and the content continues on
        // the next line. Only increment value_indent_depth if we're
        // in a "checked" value context.
        if last_meaningful_char == b'(' && value_started_on_colon_line {
          value_indent_depth += 1;
        }

        i += 1;

        // Determine if the next line is a continuation of a
        // multi-line value that should be skipped.
        // Only continuation lines where the value started on the same
        // line as the colon AND we're inside parens get indentation
        // checks. All other continuation lines are skipped.
        let is_continuation = last_meaningful_char != b';'
          && last_meaningful_char != b'{'
          && last_meaningful_char != b'}';
        let is_skippable_continuation =
          is_continuation && !(value_started_on_colon_line && paren_depth > 0 && brace_depth > 0);

        // Now check the indentation of the next line
        let expected_depth = brace_depth as usize;
        let mut actual_indent = 0;
        let mut j = line_start;
        let mut wrong_char = false;

        // Count leading whitespace
        while j < len && (bytes[j] == b' ' || bytes[j] == b'\t') {
          if use_tab {
            if bytes[j] == b'\t' {
              actual_indent += 1;
            } else {
              wrong_char = true;
            }
          } else if bytes[j] == b' ' {
            actual_indent += 1;
          } else {
            wrong_char = true;
          }
          j += 1;
        }

        // Skip empty lines
        if j >= len || bytes[j] == b'\n' || bytes[j] == b'\r' {
          continue;
        }

        // Skip continuation lines that should not be checked
        if is_skippable_continuation {
          continue;
        }

        // If the first non-whitespace char on this line is `)`, it
        // closes a value indent level.
        if j < len && bytes[j] == b')' && value_indent_depth > 0 {
          value_indent_depth -= 1;
        }

        // Compute expected indentation
        let expected = if j < len && bytes[j] == b'}' {
          // A closing brace should be at parent level
          if expected_depth > 0 {
            (expected_depth - 1) * indent_size
          } else {
            0
          }
        } else if value_indent_depth > 0 {
          // Inside multi-line parenthesized value:
          // expected = base_indent + value_indent_depth * indent_size
          expected_depth * indent_size + value_indent_depth as usize * indent_size
        } else {
          expected_depth * indent_size
        };

        if wrong_char {
          let indent_type = if use_tab { "tabs" } else { "spaces" };
          diagnostics.push(
            Diagnostic::new(
              self.name(),
              format!("Expected {indent_type} for indentation"),
            )
            .severity(self.default_severity())
            .span(Span::new(j, 0)),
          );
        } else if actual_indent != expected {
          let unit = if use_tab { "tab" } else { "space" };
          let plural = if expected != 1 { "s" } else { "" };
          diagnostics.push(
            Diagnostic::new(
              self.name(),
              format!("Expected indentation of {expected} {unit}{plural}",),
            )
            .severity(self.default_severity())
            .span(Span::new(j, 0)),
          );
        }

        continue;
      }

      // Track any other non-whitespace character as meaningful
      if bytes[i] != b' ' && bytes[i] != b'\t' && bytes[i] != b'\r' {
        last_meaningful_char = bytes[i];
        // Track colons for declaration detection
        if bytes[i] == b':' {
          colon_seen_this_line = true;
        }
        // Track @include for SCSS mixin call detection
        if bytes[i] == b'@' && i + 7 < len && &source[i..i + 8] == "@include" {
          at_include_seen_this_line = true;
        }
      }

      i += 1;
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn ctx_with_option<'a>(source: &'a str, opt: &'a serde_json::Value) -> RuleContext<'a> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: Some(opt),
    }
  }

  #[test]
  fn allows_correct_2_space_indent() {
    let opt = serde_json::json!(2);
    let source = "a {\n  color: red;\n}";
    let d = StylisticIndentation.check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn reports_wrong_indent() {
    let opt = serde_json::json!(2);
    let source = "a {\ncolor: red;\n}";
    let d = StylisticIndentation.check_root(&[], &ctx_with_option(source, &opt));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Expected indentation"));
  }

  #[test]
  fn allows_tab_indent() {
    let opt = serde_json::json!("tab");
    let source = "a {\n\tcolor: red;\n}";
    let d = StylisticIndentation.check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn reports_spaces_when_tab_expected() {
    let opt = serde_json::json!("tab");
    let source = "a {\n  color: red;\n}";
    let d = StylisticIndentation.check_root(&[], &ctx_with_option(source, &opt));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("tabs"));
  }

  #[test]
  fn allows_4_space_indent() {
    let opt = serde_json::json!(4);
    let source = "a {\n    color: red;\n}";
    let d = StylisticIndentation.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_multiline_value_continuation() {
    let opt = serde_json::json!(2);
    // Multi-line value: continuation lines after `src:` should not be checked
    let source = "@font-face {\n  src:\n    url('a.woff2') format('woff2'),\n    url('a.woff') format('woff');\n  font-weight: normal;\n}";
    let d = StylisticIndentation.check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn skips_multiline_import() {
    let opt = serde_json::json!(2);
    // Multi-line @import: continuation lines should not be checked
    let source = "@import\n  'foo',\n  'bar';";
    let d = StylisticIndentation.check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn skips_multiline_value_with_parens() {
    let opt = serde_json::json!(2);
    // Multi-line value with nested parens (calc, var)
    let source = "a {\n  margin: calc(\n    var(--x) * -1\n  );\n  color: red;\n}";
    let d = StylisticIndentation.check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn skips_multiline_transition() {
    let opt = serde_json::json!(2);
    // Multi-line transition property value
    let source = "a {\n  transition:\n    background-color 0.2s linear,\n    opacity 0.2s linear;\n  color: red;\n}";
    let d = StylisticIndentation.check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }
}
