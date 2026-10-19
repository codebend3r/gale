use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "string-no-newline";

declare_rule! {
  /// Reports literal newlines inside CSS string values.
  ///
  /// CSS strings cannot span multiple lines without escaping. An unescaped
  /// newline inside a quoted string is invalid.
  ///
  /// Equivalent to Stylelint's `string-no-newline` rule.
  pub struct StringNoNewline => StringNoNewlineCheck {
    name: NAME,
    description: "Disallow newlines in strings",
    default_severity: Warning,
  }
}

/// `string-no-newline`, whose options gale does not apply yet.
pub struct StringNoNewlineCheck;

impl StringNoNewlineCheck {
  /// Stylelint's options: nothing but `true`, then `ignore` listing
  /// `"at-rule-preludes"` and `"declaration-values"`, which gale accepts but
  /// does not yet apply.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.keywords("ignore", &["at-rule-preludes", "declaration-values"]);
    options.finish()?;
    Ok(Self)
  }
}

impl Check for StringNoNewlineCheck {
  /// Checks declaration values for unescaped newlines inside strings.
  fn check(&self, node: &CssNode, _context: &RuleContext) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if let CssNode::Style(style_rule) = node {
      for decl in &style_rule.declarations {
        check_value_for_newlines(&decl.value, decl.span.offset, &mut diagnostics);
      }
    }

    diagnostics
  }

  /// Also scans the raw source, catching strings the parser discarded as malformed.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    // Source-level scan to catch newlines in strings that the parser
    // may not preserve (e.g., lightningcss discards malformed strings).
    scan_source_for_string_newlines(ctx.source, ctx.syntax)
  }
}

/// Scan raw source text for strings containing literal newlines.
/// Handles `//` line comments (SCSS) and `/* */` block comments to avoid FPs.
fn scan_source_for_string_newlines(source: &str, syntax: Syntax) -> Vec<Diagnostic> {
  let mut diagnostics = Vec::new();
  let bytes = source.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    // Skip // line comments in SCSS/Less
    if matches!(syntax, Syntax::Scss | Syntax::Less | Syntax::Sass)
      && i + 1 < len
      && bytes[i] == b'/'
      && bytes[i + 1] == b'/'
    {
      // Skip to end of line
      while i < len && bytes[i] != b'\n' {
        i += 1;
      }
      continue;
    }

    // Skip /* */ block comments
    if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
      i += 2;
      while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
        i += 1;
      }
      i += 2;
      continue;
    }

    // Found a string delimiter
    if bytes[i] == b'"' || bytes[i] == b'\'' {
      let quote = bytes[i];
      let string_start = i;
      i += 1;
      while i < len {
        if bytes[i] == b'\\' {
          // Skip escaped character
          i += 2;
          continue;
        }
        if bytes[i] == b'\n' || bytes[i] == b'\r' {
          diagnostics.push(
            Diagnostic::new(NAME, "Unexpected newline in string")
              .span(Span::new(string_start, i - string_start)),
          );
          // Skip past the newline and break out of string
          break;
        }
        if bytes[i] == quote {
          i += 1;
          break;
        }
        i += 1;
      }
    } else {
      i += 1;
    }
  }

  diagnostics
}

/// Reports each unescaped newline inside a quoted string in `value`.
fn check_value_for_newlines(value: &str, base_offset: usize, diagnostics: &mut Vec<Diagnostic>) {
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    let b = bytes[i];
    if b == b'"' || b == b'\'' {
      let quote = b;
      let string_start = i;
      i += 1;
      while i < len {
        if bytes[i] == b'\\' {
          // Skip escaped character (including escaped newlines which are valid).
          i += 2;
          continue;
        }
        if bytes[i] == b'\n' || bytes[i] == b'\r' {
          diagnostics.push(
            Diagnostic::new(NAME, "Unexpected newline in string")
              .span(Span::new(base_offset + string_start, i - string_start)),
          );
          break;
        }
        if bytes[i] == quote {
          i += 1;
          break;
        }
        i += 1;
      }
    }
    i += 1;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, StyleRule};

  use crate::testing::{configured, ctx, invalid_options};

  #[test]
  fn reports_newline_in_string_value() {
    let rule = configured(&StringNoNewline, None);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "content".to_string(),
        value: "\"hello\nworld\"".to_string(),
        span: Span::new(4, 20),
        important: false,
      }],
      span: Span::new(0, 30),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].message, "Unexpected newline in string");
  }

  #[test]
  fn ignores_string_without_newline() {
    let rule = configured(&StringNoNewline, None);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "content".to_string(),
        value: "\"hello world\"".to_string(),
        span: Span::new(4, 20),
        important: false,
      }],
      span: Span::new(0, 30),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_escaped_newline() {
    let rule = configured(&StringNoNewline, None);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "content".to_string(),
        value: "\"hello\\\nworld\"".to_string(),
        span: Span::new(4, 20),
        important: false,
      }],
      span: Span::new(0, 30),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignore": ["at-rule-preludes", "declaration-values"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"string-no-newline\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignore": ["comments"] }])),
      ["Invalid value \"comments\" for option \"ignore\" of rule \"string-no-newline\""]
    );
  }
}
