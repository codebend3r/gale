use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::braces::is_block_multiline;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/declaration-block-semicolon-space-after";

declare_rule! {
  /// Require or disallow a space after the semicolons of declaration blocks.
  ///
  /// Equivalent to `@stylistic/declaration-block-semicolon-space-after`.
  pub struct StylisticDeclarationBlockSemicolonSpaceAfter => StylisticDeclarationBlockSemicolonSpaceAfterCheck {
    name: NAME,
    description: "Require or disallow a space after the semicolons of declaration blocks",
    default_severity: Warning,
  }
}

/// `@stylistic/declaration-block-semicolon-space-after` as configured.
pub struct StylisticDeclarationBlockSemicolonSpaceAfterCheck {
  /// The primary option: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`.
  option: &'static str,
}

impl StylisticDeclarationBlockSemicolonSpaceAfterCheck {
  /// The plugin's options: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`.  Gale reads no option as `"always-single-line"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option =
      options.primary_one_of(&["always", "never", "always-single-line", "never-single-line"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always-single-line"),
    })
  }
}

impl Check for StylisticDeclarationBlockSemicolonSpaceAfterCheck {
  /// Flags a declaration semicolon whose trailing space does not match the option.
  fn check(&self, _tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;
    let mut depth: i32 = 0;

    while i < len {
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
        if i < len {
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

      // Skip SCSS line comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }

      if bytes[i] == b'{' {
        depth += 1;
        i += 1;
        continue;
      }

      if bytes[i] == b'}' {
        depth -= 1;
        if depth < 0 {
          depth = 0;
        }
        i += 1;
        continue;
      }

      // Check semicolons inside blocks
      if bytes[i] == b';' && depth > 0 {
        let semi_pos = i;

        // Look at the character after the semicolon (skip whitespace for analysis)
        let j = i + 1;

        // If next non-whitespace is `}`, skip (last declaration)
        let mut peek = j;
        while peek < len
          && (bytes[peek] == b' '
            || bytes[peek] == b'\t'
            || bytes[peek] == b'\n'
            || bytes[peek] == b'\r')
        {
          peek += 1;
        }
        if peek >= len || bytes[peek] == b'}' {
          i += 1;
          continue;
        }

        // Check the immediate character after `;`
        let has_space = j < len && bytes[j] == b' ';

        let is_single_line = !is_block_multiline(bytes, semi_pos);

        let violation = match option {
          "always" => !has_space,
          "never" => has_space,
          "always-single-line" => is_single_line && !has_space,
          "never-single-line" => is_single_line && has_space,
          _ => false,
        };

        if violation {
          let msg = match option {
            "always" | "always-single-line" => "Expected single space after \";\"",
            "never" | "never-single-line" => "Unexpected whitespace after \";\"",
            _ => unreachable!(),
          };
          diagnostics.push(Diagnostic::new(NAME, msg).span(Span::new(semi_pos, 1)));
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

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  #[test]
  fn always_single_line_allows_space_after_semicolon() {
    let opt = serde_json::json!("always-single-line");
    let rule = configured(&StylisticDeclarationBlockSemicolonSpaceAfter, opt.clone());
    let source = "a { color: red; display: block; }";
    let d = rule.check_source(&ctx_with_source(source));
    assert!(
      d.is_empty(),
      "got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn always_single_line_reports_missing_space() {
    let opt = serde_json::json!("always-single-line");
    let rule = configured(&StylisticDeclarationBlockSemicolonSpaceAfter, opt.clone());
    let source = "a { color: red;display: block; }";
    let d = rule.check_source(&ctx_with_source(source));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Expected single space"));
  }

  #[test]
  fn always_single_line_ignores_multiline_block() {
    let opt = serde_json::json!("always-single-line");
    let rule = configured(&StylisticDeclarationBlockSemicolonSpaceAfter, opt.clone());
    let source = "a {\n  color: red;display: block;\n}";
    let d = rule.check_source(&ctx_with_source(source));
    assert!(
      d.is_empty(),
      "should not report in multi-line block, got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn never_reports_space_after_semicolon() {
    let opt = serde_json::json!("never");
    let rule = configured(&StylisticDeclarationBlockSemicolonSpaceAfter, opt.clone());
    let source = "a {\n  color: red; display: block;\n}";
    let d = rule.check_source(&ctx_with_source(source));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Unexpected whitespace"));
  }

  #[test]
  fn always_requires_space_after_semicolon() {
    let opt = serde_json::json!("always");
    let rule = configured(&StylisticDeclarationBlockSemicolonSpaceAfter, opt.clone());
    let source = "a { color: red;display: block; }";
    let d = rule.check_source(&ctx_with_source(source));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Expected single space"));
  }

  #[test]
  fn skips_last_declaration_before_closing_brace() {
    let opt = serde_json::json!("always");
    let rule = configured(&StylisticDeclarationBlockSemicolonSpaceAfter, opt.clone());
    let source = "a { color: red;}";
    let d = rule.check_source(&ctx_with_source(source));
    assert!(
      d.is_empty(),
      "should skip semicolon before }}, got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn never_single_line_reports_space_in_single_line() {
    let opt = serde_json::json!("never-single-line");
    let rule = configured(&StylisticDeclarationBlockSemicolonSpaceAfter, opt.clone());
    let source = "a { color: red; display: block; }";
    let d = rule.check_source(&ctx_with_source(source));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Unexpected whitespace"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("never-single-line")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always-multi-line")),
      [
        "Invalid option value \"always-multi-line\" for rule \"@stylistic/declaration-block-semicolon-space-after\""
      ]
    );
  }
}
