use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/selector-list-comma-newline-after";

declare_rule! {
  /// Require a newline or disallow whitespace after the commas of selector lists.
  ///
  /// Equivalent to `@stylistic/selector-list-comma-newline-after`.
  pub struct StylisticSelectorListCommaNewlineAfter => StylisticSelectorListCommaNewlineAfterCheck {
    name: NAME,
    description: "Require a newline or disallow whitespace after the commas of selector lists",
    default_severity: Warning,
  }
}

/// `@stylistic/selector-list-comma-newline-after` as configured.
pub struct StylisticSelectorListCommaNewlineAfterCheck {
  /// The primary option: `"always"`, `"always-multi-line"`,
  /// `"never-multi-line"` or gale's own `"never"`.
  option: &'static str,
}

impl StylisticSelectorListCommaNewlineAfterCheck {
  /// The plugin's options: `"always"`, `"always-multi-line"` or
  /// `"never-multi-line"`.  Gale also accepts `"never"`, which the plugin does
  /// not.  Gale reads no option as `"always-multi-line"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option =
      options.primary_one_of(&["always", "always-multi-line", "never-multi-line", "never"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always-multi-line"),
    })
  }
}

impl Check for StylisticSelectorListCommaNewlineAfterCheck {
  /// Flags a selector list comma whose trailing newline does not match the option.
  ///
  /// Reads the selector of every standard-syntax rule as written, as the
  /// plugin does: a list is multi-line when it is written over more than one
  /// line.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let mut diags = Vec::new();
    for rule in tree.walk_rules().filter(|rule| rule.is_standard_syntax()) {
      let selector = rule.raw_selector();
      // Check if this is a selector list (contains commas)
      if selector.contains(',') {
        let is_multi_line = selector.contains('\n');
        check_commas(
          &selector,
          rule.selector_start(),
          is_multi_line,
          option,
          &mut diags,
        );
      }
    }
    diags
  }
}

/// Reports the commas of `sel_source`, a selector list starting at
/// `sel_offset` in the source, whose trailing newline does not match
/// `option`.
fn check_commas(
  sel_source: &str,
  sel_offset: usize,
  is_multi_line: bool,
  option: &str,
  diags: &mut Vec<Diagnostic>,
) {
  {
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

      // Skip block comments: a comma inside one is not a list separator.
      if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
        i = sel_source[i + 2..]
          .find("*/")
          .map_or(len, |end| i + 2 + end + 2);
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
          diags.push(Diagnostic::new(NAME, msg).span(Span::new(sel_offset + i, 1)));
        }
      }
      i += 1;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{
    CheckSource, configured, ctx_with_source, invalid_options, scss_ctx_with_source,
  };

  /// An empty rule with the selector `sel`.
  fn style_with_selector(sel: &str) -> String {
    format!("{sel} {{}}")
  }

  #[test]
  fn reports_missing_newline_after_comma_multiline() {
    let rule = configured(&StylisticSelectorListCommaNewlineAfter, None);
    let sel = "a,\nb, c";
    let d = rule.check_source(&ctx_with_source(&style_with_selector(sel)));
    // The second comma (b, c) has a space instead of newline
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("newline"));
  }

  #[test]
  fn allows_newline_after_all_commas() {
    let rule = configured(&StylisticSelectorListCommaNewlineAfter, None);
    let sel = "a,\nb,\nc";
    let d = rule.check_source(&ctx_with_source(&style_with_selector(sel)));
    assert!(d.is_empty());
  }

  #[test]
  fn allows_scss_line_comment_after_comma() {
    let rule = configured(&StylisticSelectorListCommaNewlineAfter, None);
    let sel = "&[aria-disabled=\"true\"]:enabled, // This catches a situation\n&[aria-disabled=\"true\"]:active:enabled";
    let d = rule.check_source(&scss_ctx_with_source(&style_with_selector(sel)));
    assert!(
      d.is_empty(),
      "Should not flag comma followed by SCSS line comment, got {} diagnostics",
      d.len()
    );
  }

  #[test]
  fn allows_block_comment_newline_after_comma() {
    let rule = configured(&StylisticSelectorListCommaNewlineAfter, None);
    let sel = "a, /* comment */\nb,\nc";
    let d = rule.check_source(&ctx_with_source(&style_with_selector(sel)));
    assert!(
      d.is_empty(),
      "Should not flag comma followed by block comment then newline, got {} diagnostics",
      d.len()
    );
  }

  #[test]
  fn allows_single_line_with_default() {
    let rule = configured(&StylisticSelectorListCommaNewlineAfter, None);
    let sel = "a, b, c";
    let d = rule.check_source(&ctx_with_source(&style_with_selector(sel)));
    // Default is "always-multi-line", single line is fine
    assert!(d.is_empty());
  }

  #[test]
  fn a_list_written_over_several_lines_is_multi_line_in_css_too() {
    let rule = configured(&StylisticSelectorListCommaNewlineAfter, None);
    let source = "a { b,\n  c, d {} }";
    let d = rule.check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].span, Span::new(source.find(", d").unwrap(), 1));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("always-multi-line")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never-multi-line")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/selector-list-comma-newline-after\""
      ]
    );
  }
}
