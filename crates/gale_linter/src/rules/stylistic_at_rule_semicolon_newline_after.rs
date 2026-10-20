use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/at-rule-semicolon-newline-after";

declare_rule! {
  /// Require a newline after the semicolon of at-rules.
  ///
  /// Equivalent to Stylelint's `@stylistic/at-rule-semicolon-newline-after` rule.
  pub struct StylisticAtRuleSemicolonNewlineAfter => StylisticAtRuleSemicolonNewlineAfterCheck {
    name: NAME,
    description: "Require a newline after the semicolon of at-rules",
    default_severity: Warning,
  }
}

/// `@stylistic/at-rule-semicolon-newline-after`, whose only option is
/// `"always"`.
pub struct StylisticAtRuleSemicolonNewlineAfterCheck;

impl StylisticAtRuleSemicolonNewlineAfterCheck {
  /// The plugin's options: `"always"`, the only one.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.primary_one_of(&["always"]);
    options.finish()?;
    Ok(Self)
  }
}

impl Check for StylisticAtRuleSemicolonNewlineAfterCheck {
  /// Flags an at-rule semicolon not followed by a newline.
  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;
    let mut in_at_rule = false;
    // `//` starts a comment in SCSS and Less, except inside parentheses,
    // where it is part of a value such as `url(//example.com/a.css)`.
    let line_comments = !matches!(context.syntax, gale_css_parser::Syntax::Css);
    let mut paren_depth = 0usize;

    while i < len {
      if bytes[i] == b'(' {
        paren_depth += 1;
      } else if bytes[i] == b')' {
        paren_depth = paren_depth.saturating_sub(1);
      }

      // Skip line comments, so an `@todo` in one is no at-rule.
      if line_comments && paren_depth == 0 && bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }

      // Skip strings
      if bytes[i] == b'"' || bytes[i] == b'\'' {
        let quote = bytes[i];
        i += 1;
        while i < len && bytes[i] != quote {
          if bytes[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
        i += 1;
        continue;
      }

      // Skip comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        i += 2;
        continue;
      }

      if bytes[i] == b'@' {
        in_at_rule = true;
        i += 1;
        continue;
      }

      if bytes[i] == b'{' {
        // At-rules with blocks don't end with semicolons
        in_at_rule = false;
        i += 1;
        continue;
      }

      if bytes[i] == b';' && in_at_rule {
        let semi_pos = i;
        in_at_rule = false;
        i += 1;

        // Check what follows the semicolon
        if i >= len {
          // EOF after semicolon is fine
          continue;
        }

        // Skip spaces/tabs, then expect newline
        let mut j = i;
        let mut found_newline = false;
        let mut found_non_ws = false;
        while j < len {
          if bytes[j] == b'\n' || bytes[j] == b'\r' {
            found_newline = true;
            break;
          } else if bytes[j] == b' ' || bytes[j] == b'\t' {
            j += 1;
          } else {
            found_non_ws = true;
            break;
          }
        }

        // SCSS line comment after `;` counts as having a newline
        if j + 1 < len && bytes[j] == b'/' && bytes[j + 1] == b'/' {
          found_newline = true;
        }
        // If the next non-ws character is `}`, the at-rule is the last
        // statement in the block — no newline needed.
        if found_non_ws && !found_newline && j < len && bytes[j] != b'}' {
          diagnostics.push(
            Diagnostic::new(NAME, "Expected newline after \";\" of at-rule".to_string())
              .span(Span::new(semi_pos, 1)),
          );
        }
      } else {
        if bytes[i] == b';' || bytes[i] == b'}' {
          in_at_rule = false;
        }
        i += 1;
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{configured, ctx_with_source, invalid_options, scss_ctx_with_source};

  #[test]
  fn allows_newline_after_at_rule_semicolon() {
    let rule = configured(&StylisticAtRuleSemicolonNewlineAfter, None);
    let source = "@import url(\"foo.css\");\na { }";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_missing_newline_after_at_rule_semicolon() {
    let rule = configured(&StylisticAtRuleSemicolonNewlineAfter, None);
    let source = "@import url(\"foo.css\"); a { }";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected newline"));
  }

  #[test]
  fn at_signs_in_scss_line_comments_are_no_at_rules() {
    let rule = configured(&StylisticAtRuleSemicolonNewlineAfter, None);
    let source = "a {\n\t// @todo refactor this away; it is\n\t// in the quote block.\n\t&:first-child { color: red; }\n}\n@import url(//example.com/a.css); b { }";
    let ctx = scss_ctx_with_source(source);
    let d = rule.check_root(&[], &ctx);
    // Only the `@import`, whose `//` sits inside `url()`.
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].span.offset, source.rfind(';').unwrap());
  }

  #[test]
  fn allows_eof_after_semicolon() {
    let rule = configured(&StylisticAtRuleSemicolonNewlineAfter, None);
    let source = "@charset \"UTF-8\";";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("never")),
      ["Invalid option value \"never\" for rule \"@stylistic/at-rule-semicolon-newline-after\""]
    );
  }
}
