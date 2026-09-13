use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

/// Disallow specific property-value pairs.
///
/// Options: a JSON object where keys are property names or `/regex/` patterns,
/// and values are arrays of disallowed value strings or `/regex/` patterns.
///
/// Example:
///   { "/^border/": ["none"], "/^transition/": ["/all/"] }
///
/// Equivalent to Stylelint's `declaration-property-value-disallowed-list` rule.
pub struct DeclarationPropertyValueDisallowedList;

impl Rule for DeclarationPropertyValueDisallowedList {
  fn name(&self) -> &'static str {
    "declaration-property-value-disallowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of disallowed property and value pairs within declarations"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let Some(opts) = ctx.primary_option() else {
      return vec![];
    };
    let Some(map) = opts.as_object() else {
      return vec![];
    };
    if map.is_empty() {
      return vec![];
    }

    // Pre-compile the disallowed list from config.
    let mut pairs: Vec<(PropertyMatcher, Vec<ValueMatcher>)> = Vec::new();
    for (prop_pattern, disallowed_values) in map {
      let prop_matcher = PropertyMatcher::new(prop_pattern);
      let Some(values_arr) = disallowed_values.as_array() else {
        continue;
      };
      let value_matchers: Vec<ValueMatcher> = values_arr
        .iter()
        .filter_map(|v| v.as_str().map(ValueMatcher::new))
        .collect();
      if !value_matchers.is_empty() {
        pairs.push((prop_matcher, value_matchers));
      }
    }

    if pairs.is_empty() {
      return vec![];
    }

    // Scan the source for declarations using byte scanning.
    // This approach handles SCSS/Less and nested rules correctly.
    let source = ctx.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;
    let mut paren_depth: i32 = 0;

    while i < len {
      // Track parens (colons inside parens are selector pseudo-classes)
      if bytes[i] == b'(' {
        paren_depth += 1;
      } else if bytes[i] == b')' {
        paren_depth -= 1;
        if paren_depth < 0 {
          paren_depth = 0;
        }
      }

      // Skip comments
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
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }

      // Skip strings
      if bytes[i] == b'\'' || bytes[i] == b'"' {
        let q = bytes[i];
        i += 1;
        while i < len && bytes[i] != q {
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

      // Skip selectors / at-rules — look for property: value;
      // A declaration starts after a `{` or `;` or start of line inside a block.
      // We look for `property: value` pattern.
      // Skip colons inside parens (pseudo-classes in selectors like `:has(input:invalid)`)
      if bytes[i] == b':' && paren_depth == 0 {
        // Walk backward to find the property name
        let colon = i;
        let mut prop_end = colon;
        let mut j = colon.wrapping_sub(1);
        // Skip whitespace before colon
        while j < len && (bytes[j] == b' ' || bytes[j] == b'\t') {
          j = j.wrapping_sub(1);
        }
        if j >= len {
          i += 1;
          continue;
        }
        prop_end = j + 1;
        // Walk back over the property name
        while j < len && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-' || bytes[j] == b'_')
        {
          j = j.wrapping_sub(1);
        }
        let prop_start = j.wrapping_add(1);
        if prop_start >= prop_end || prop_start >= len {
          i += 1;
          continue;
        }

        // Check if this is actually a property (preceded by whitespace, newline, { or ;)
        // and not a pseudo-selector or part of a selector
        let before_prop = if prop_start > 0 {
          bytes[prop_start - 1]
        } else {
          b'\n'
        };
        let is_declaration = before_prop == b' '
          || before_prop == b'\t'
          || before_prop == b'\n'
          || before_prop == b'\r'
          || before_prop == b'{'
          || before_prop == b';';

        if !is_declaration {
          i += 1;
          continue;
        }

        let property = &source[prop_start..prop_end];

        // Check if any property pattern matches
        let mut matching_pairs: Vec<&Vec<ValueMatcher>> = Vec::new();
        for (prop_matcher, val_matchers) in &pairs {
          if prop_matcher.matches(property) {
            matching_pairs.push(val_matchers);
          }
        }

        if matching_pairs.is_empty() {
          i += 1;
          continue;
        }

        // Find the value (after `:` to `;` or `}` or newline for last decl)
        let mut val_start = colon + 1;
        while val_start < len && (bytes[val_start] == b' ' || bytes[val_start] == b'\t') {
          val_start += 1;
        }
        let mut val_end = val_start;
        let mut paren_depth = 0i32;
        while val_end < len {
          if bytes[val_end] == b'(' {
            paren_depth += 1;
          } else if bytes[val_end] == b')' {
            paren_depth -= 1;
          } else if paren_depth == 0 {
            if bytes[val_end] == b';' || bytes[val_end] == b'}' || bytes[val_end] == b'{' {
              break;
            }
          }
          val_end += 1;
        }

        let value = source[val_start..val_end].trim();

        // Check each matching property pattern's disallowed values
        for val_matchers in &matching_pairs {
          for matcher in *val_matchers {
            if matcher.matches(value) {
              diagnostics.push(
                Diagnostic::new(
                  self.name(),
                  format!("Unexpected value \"{value}\" for property \"{property}\""),
                )
                .severity(self.default_severity())
                .span(Span::new(prop_start, 0)),
              );
              break;
            }
          }
        }

        i = val_end;
        continue;
      }

      i += 1;
    }

    diagnostics
  }
}

/// Matches a property name — either exact string or regex pattern.
enum PropertyMatcher {
  Exact(String),
  Regex(Regex),
}

impl PropertyMatcher {
  fn new(pattern: &str) -> Self {
    if pattern.starts_with('/') && pattern.len() > 1 {
      if let Some(end) = pattern[1..].rfind('/') {
        let re_str = &pattern[1..1 + end];
        let flags = &pattern[2 + end..];
        let full = if flags.contains('i') {
          format!("(?i){re_str}")
        } else {
          re_str.to_string()
        };
        if let Ok(re) = Regex::new(&full) {
          return Self::Regex(re);
        }
      }
    }
    Self::Exact(pattern.to_string())
  }

  fn matches(&self, property: &str) -> bool {
    match self {
      Self::Exact(s) => property == s.as_str(),
      Self::Regex(re) => re.is_match(property),
    }
  }
}

/// Matches a value — either exact string or regex pattern.
enum ValueMatcher {
  Exact(String),
  Regex(Regex),
}

impl ValueMatcher {
  fn new(pattern: &str) -> Self {
    if pattern.starts_with('/') && pattern.len() > 1 {
      if let Some(end) = pattern[1..].rfind('/') {
        let re_str = &pattern[1..1 + end];
        let flags = &pattern[2 + end..];
        let full = if flags.contains('i') {
          format!("(?i){re_str}")
        } else {
          re_str.to_string()
        };
        if let Ok(re) = Regex::new(&full) {
          return Self::Regex(re);
        }
      }
    }
    Self::Exact(pattern.to_string())
  }

  fn matches(&self, value: &str) -> bool {
    match self {
      Self::Exact(s) => value == s.as_str(),
      Self::Regex(re) => re.is_match(value),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn check(source: &str, opts: &serde_json::Value) -> Vec<Diagnostic> {
    let rule = DeclarationPropertyValueDisallowedList;
    let ctx = RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: Some(opts),
    };
    rule.check_root(&[], &ctx)
  }

  #[test]
  fn detects_border_none() {
    let opts = serde_json::json!({"/^border/": ["none"]});
    let d = check("a { border: none; }", &opts);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("none"));
    assert!(d[0].message.contains("border"));
  }

  #[test]
  fn detects_border_top_none() {
    let opts = serde_json::json!({"/^border/": ["none"]});
    let d = check("a { border-top: none; }", &opts);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_border_solid() {
    let opts = serde_json::json!({"/^border/": ["none"]});
    let d = check("a { border: 1px solid red; }", &opts);
    assert!(d.is_empty());
  }

  #[test]
  fn detects_transition_all() {
    let opts = serde_json::json!({"/^transition/": ["/all/"]});
    let d = check("a { transition: all 0.3s ease; }", &opts);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_when_no_options() {
    let rule = DeclarationPropertyValueDisallowedList;
    let ctx = RuleContext {
      file_path: "test.css",
      source: "a { border: none; }",
      syntax: Syntax::Css,
      options: None,
    };
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }
}
