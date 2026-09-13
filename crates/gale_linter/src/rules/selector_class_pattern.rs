use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

const DEFAULT_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z0-9]+)*$";

/// Enforces a pattern for class selectors.
///
/// Accepts a regex string as the primary option (e.g. `"^[a-z][a-zA-Z0-9]+$"`).
/// Defaults to kebab-case pattern if no option is provided.
///
/// Equivalent to Stylelint's `selector-class-pattern` rule.
pub struct SelectorClassPattern;

impl Rule for SelectorClassPattern {
  fn name(&self) -> &'static str {
    "selector-class-pattern"
  }

  fn description(&self) -> &'static str {
    "Specify a pattern for class selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    // Read the user-supplied regex pattern from options, or use the default kebab-case pattern.
    // Options may be a plain string "^pattern$" or an array ["^pattern$", { secondary }].
    let pattern_str = ctx.primary_option_str().unwrap_or(DEFAULT_PATTERN);

    let re = match Regex::new(pattern_str) {
      Ok(r) => r,
      Err(_) => return vec![],
    };

    let mut diags = Vec::new();

    // In Stylelint (with postcss-scss), when a selector contains SCSS
    // interpolation `#{...}`, the entire selector is skipped because the
    // resolved class name is unknown at lint time.  Match this behaviour
    // by skipping all class checks when the selector contains interpolation.
    if matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass
    ) && rule.selector.contains("#{")
    {
      return diags;
    }

    // Stylelint checks each individual selector in a comma-separated
    // list and reports per-class violations with the position of the
    // class within the source.  We split by comma, extract classes from
    // each part, and compute byte offsets for accurate reporting.
    let selector_start = rule.span.offset;
    let parts = split_selector_list(&rule.selector);
    let mut cursor: usize = 0; // byte position within the selector string

    for (idx, part) in parts.iter().enumerate() {
      if idx > 0 {
        // Skip past the comma separator in the selector string.
        cursor += 1; // the comma
      }
      // Skip whitespace between comma and start of this part.
      let sel_bytes = rule.selector.as_bytes();
      while cursor < sel_bytes.len() && matches!(sel_bytes[cursor], b' ' | b'\t' | b'\n' | b'\r') {
        cursor += 1;
      }
      let part_start = cursor;
      let trimmed_part = part.trim();
      // Advance cursor past this part.
      cursor = part_start + trimmed_part.len();

      for (class, class_byte_offset) in extract_class_names_with_offsets(trimmed_part) {
        if !re.is_match(&class) {
          let offset = selector_start + part_start + class_byte_offset;
          let msg = if is_kebab_case_pattern(pattern_str) {
            format!("Expected class selector \".{class}\" to be kebab-case")
          } else {
            format!("Expected \".{class}\" to match pattern \"{pattern_str}\"")
          };
          diags.push(
            Diagnostic::new(self.name(), msg)
              .severity(self.default_severity())
              .span(Span::new(offset, class.len() + 1)), // +1 for the dot
          );
        }
      }
    }
    diags
  }
}

/// Returns true if the pattern is the default kebab-case pattern.
/// Stylelint uses "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$" as the default,
/// and produces a human-readable "to be kebab-case" message for it.
fn is_kebab_case_pattern(pattern: &str) -> bool {
  pattern == DEFAULT_PATTERN || pattern == "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$"
}

/// Split a selector list by commas, respecting parentheses and brackets.
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

/// Extract class names from a selector along with their byte offsets
/// (offset of the `.` character) within the selector string.
fn extract_class_names_with_offsets(selector: &str) -> Vec<(String, usize)> {
  let mut classes = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut byte_pos: usize = 0;

  while i < len {
    let char_byte_len = chars[i].len_utf8();
    if chars[i] == '.' {
      let dot_byte_pos = byte_pos;
      i += 1;
      byte_pos += char_byte_len;
      let start = i;
      // A CSS class selector must start with a letter, hyphen, underscore,
      // or non-ASCII character after the dot. If the next character is a
      // digit, this is a numeric value (e.g. `.5` in `opacity: .5`), not
      // a class selector.
      if i < len && chars[i].is_ascii_digit() {
        // Skip past the numeric value
        while i < len
          && (chars[i].is_ascii_alphanumeric()
            || chars[i] == '.'
            || chars[i] == '-'
            || chars[i] == '_')
        {
          byte_pos += chars[i].len_utf8();
          i += 1;
        }
        continue;
      }
      while i < len {
        if chars[i].is_ascii_alphanumeric()
          || chars[i] == '-'
          || chars[i] == '_'
          || !chars[i].is_ascii()
        {
          byte_pos += chars[i].len_utf8();
          i += 1;
        } else if chars[i] == '#' && i + 1 < len && chars[i + 1] == '{' {
          byte_pos += chars[i].len_utf8();
          i += 1;
          byte_pos += chars[i].len_utf8();
          i += 1;
          let mut depth = 1;
          while i < len && depth > 0 {
            if chars[i] == '{' {
              depth += 1;
            } else if chars[i] == '}' {
              depth -= 1;
            }
            byte_pos += chars[i].len_utf8();
            i += 1;
          }
        } else {
          break;
        }
      }
      if i > start {
        let name: String = chars[start..i].iter().collect();
        classes.push((name, dot_byte_pos));
      }
    } else {
      byte_pos += char_byte_len;
      i += 1;
    }
  }

  classes
}

fn extract_class_names(selector: &str) -> Vec<String> {
  let mut classes = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    if chars[i] == '.' {
      i += 1;
      let start = i;
      // A CSS class selector must start with a letter, hyphen, underscore,
      // or non-ASCII character after the dot. If the next character is a
      // digit, this is a numeric value (e.g. `.5` in `opacity: .5`), not
      // a class selector.
      if i < len && chars[i].is_ascii_digit() {
        while i < len
          && (chars[i].is_ascii_alphanumeric()
            || chars[i] == '.'
            || chars[i] == '-'
            || chars[i] == '_')
        {
          i += 1;
        }
        continue;
      }
      // CSS class: ident chars (alphanum, hyphen, underscore, non-ASCII)
      // Also consume SCSS interpolation #{...} within class names
      while i < len {
        if chars[i].is_ascii_alphanumeric()
          || chars[i] == '-'
          || chars[i] == '_'
          || !chars[i].is_ascii()
        {
          i += 1;
        } else if chars[i] == '#' && i + 1 < len && chars[i + 1] == '{' {
          // Consume SCSS interpolation #{...}
          i += 2; // skip #{
          let mut depth = 1;
          while i < len && depth > 0 {
            if chars[i] == '{' {
              depth += 1;
            } else if chars[i] == '}' {
              depth -= 1;
            }
            i += 1;
          }
        } else {
          break;
        }
      }
      if i > start {
        classes.push(chars[start..i].iter().collect());
      }
    } else {
      i += 1;
    }
  }

  classes
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
  fn reports_camel_case_class() {
    let d = SelectorClassPattern.check(&style_with_selector(".myClass"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myClass"));
  }

  #[test]
  fn allows_kebab_case_class() {
    assert!(
      SelectorClassPattern
        .check(&style_with_selector(".my-class"), &ctx())
        .is_empty()
    );
    assert!(
      SelectorClassPattern
        .check(&style_with_selector(".foo"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_underscore_class() {
    let d = SelectorClassPattern.check(&style_with_selector(".my_class"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn custom_pattern_camel_case() {
    let opts = serde_json::json!("^[a-z][a-zA-Z0-9]+$");
    let c = ctx_with_options(&opts);
    // camelCase should pass
    assert!(
      SelectorClassPattern
        .check(&style_with_selector(".myClass"), &c)
        .is_empty()
    );
    // kebab-case should fail
    let d = SelectorClassPattern.check(&style_with_selector(".my-class"), &c);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn custom_pattern_in_message() {
    let opts = serde_json::json!("^[a-z][a-zA-Z0-9]+$");
    let c = ctx_with_options(&opts);
    let d = SelectorClassPattern.check(&style_with_selector(".my-class"), &c);
    assert!(d[0].message.contains("^[a-z][a-zA-Z0-9]+$"));
  }

  #[test]
  fn ignores_numeric_values_after_dot() {
    // These are numeric values (e.g. `opacity: .5`), not class selectors.
    // Gale should not report them as class pattern violations.
    let c = ctx();
    assert!(
      SelectorClassPattern
        .check(&style_with_selector(".5"), &c)
        .is_empty(),
      ".5 should be ignored (numeric value)"
    );
    assert!(
      SelectorClassPattern
        .check(&style_with_selector(".2"), &c)
        .is_empty(),
      ".2 should be ignored (numeric value)"
    );
    assert!(
      SelectorClassPattern
        .check(&style_with_selector(".15"), &c)
        .is_empty(),
      ".15 should be ignored (numeric value)"
    );
    assert!(
      SelectorClassPattern
        .check(&style_with_selector(".65"), &c)
        .is_empty(),
      ".65 should be ignored (numeric value)"
    );
    assert!(
      SelectorClassPattern
        .check(&style_with_selector(".8"), &c)
        .is_empty(),
      ".8 should be ignored (numeric value)"
    );
  }

  #[test]
  fn extract_class_names_skips_numeric_dot_values() {
    // Verify the extraction function itself skips numeric values
    assert!(extract_class_names(".5").is_empty());
    assert!(extract_class_names(".15").is_empty());
    assert!(extract_class_names(".8").is_empty());
    // But real class names still work
    assert_eq!(extract_class_names(".foo"), vec!["foo"]);
    assert_eq!(extract_class_names(".foo-bar"), vec!["foo-bar"]);
  }
}
