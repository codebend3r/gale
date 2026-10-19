use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/dollar-variable-empty-line-before";

declare_rule! {
  /// Require or disallow an empty line before `$`-variable declarations.
  ///
  /// Primary option: `"always"` or `"never"`.
  ///
  /// Secondary options:
  /// - `except: ["after-comment", "after-dollar-variable", "first-nested"]`
  /// - `ignore: ["after-comment", "inside-single-line-block"]`
  ///
  /// Equivalent to `scss/dollar-variable-empty-line-before`.
  pub struct ScssDollarVariableEmptyLineBefore => ScssDollarVariableEmptyLineBeforeCheck {
    name: NAME,
    description: "Require or disallow an empty line before $-variable declarations",
    default_severity: Warning,
  }
}

/// `scss/dollar-variable-empty-line-before` as configured.
pub struct ScssDollarVariableEmptyLineBeforeCheck {
  /// The primary option: `"always"` or `"never"`.
  expectation: &'static str,
  /// `except: ["after-comment"]`.
  except_after_comment: bool,
  /// `except: ["after-dollar-variable"]`.
  except_after_dollar_variable: bool,
  /// `except: ["first-nested"]`.
  except_first_nested: bool,
  /// `ignore: ["after-comment"]`.
  ignore_after_comment: bool,
  /// `ignore: ["after-dollar-variable"]`.
  ignore_after_dollar_variable: bool,
  /// `ignore: ["inside-single-line-block"]`.
  ignore_inside_single_line_block: bool,
}

impl ScssDollarVariableEmptyLineBeforeCheck {
  /// Stylelint's options: `"always"` or `"never"` (`"always"` when not
  /// given), then `except`, `ignore` and `disableFix`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expectation = options
      .primary_one_of(&["always", "never"])
      .unwrap_or("always");
    let except = options.keywords(
      "except",
      &["first-nested", "after-comment", "after-dollar-variable"],
    );
    let ignore = options.keywords(
      "ignore",
      &[
        "after-comment",
        "inside-single-line-block",
        "after-dollar-variable",
      ],
    );
    options.disable_fix();
    options.finish()?;
    Ok(Self {
      expectation,
      except_after_comment: except.contains(&"after-comment"),
      except_after_dollar_variable: except.contains(&"after-dollar-variable"),
      except_first_nested: except.contains(&"first-nested"),
      ignore_after_comment: ignore.contains(&"after-comment"),
      ignore_after_dollar_variable: ignore.contains(&"after-dollar-variable"),
      ignore_inside_single_line_block: ignore.contains(&"inside-single-line-block"),
    })
  }
}

impl Check for ScssDollarVariableEmptyLineBeforeCheck {
  /// Flags a `$variable` declaration whose preceding blank line does not match
  /// the option, after applying the `except`/`ignore` secondaries.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let Self {
      expectation,
      except_after_comment,
      except_after_dollar_variable,
      except_first_nested,
      ignore_after_comment,
      ignore_after_dollar_variable,
      ignore_inside_single_line_block,
    } = *self;

    let source = ctx.source;
    let lines: Vec<&str> = source.lines().collect();
    let mut diagnostics = Vec::new();

    // Track parenthesis depth to skip $var: inside mixin arguments,
    // @use ... with (...), and function calls.
    let mut paren_depth: i32 = 0;

    for (line_idx, line) in lines.iter().enumerate() {
      // Update paren depth based on this line's content.
      // We need to count parens outside of strings/comments.
      for ch in line.chars() {
        if ch == '(' {
          paren_depth += 1;
        } else if ch == ')' {
          paren_depth -= 1;
          if paren_depth < 0 {
            paren_depth = 0;
          }
        }
      }

      let trimmed = line.trim();

      // Only look at lines that start with a $variable declaration
      if !trimmed.starts_with('$') || !trimmed.contains(':') {
        continue;
      }

      // Skip if we're inside parentheses (mixin args, @use with, etc.)
      if paren_depth > 0 {
        continue;
      }

      // Check that the part before `:` is a valid variable name
      let before_colon = trimmed.split(':').next().unwrap_or("");
      if !before_colon.starts_with('$')
        || !before_colon[1..]
          .chars()
          .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
      {
        continue;
      }

      // Also skip if the line ends with a comma (likely a mixin argument)
      // or if looking at source context indicates we're inside a
      // multi-line paren expression.
      if trimmed.ends_with(',') {
        // Check if this is a parameter (common in @include, @use with)
        // by looking for an unclosed `(` in previous lines.
        let byte_offset: usize = lines[..line_idx].iter().map(|l| l.len() + 1).sum();
        let before = &source[..byte_offset];
        let open_parens = before.chars().filter(|&c| c == '(').count();
        let close_parens = before.chars().filter(|&c| c == ')').count();
        if open_parens > close_parens {
          continue;
        }
      }

      // Determine context of preceding line
      let is_first_line = line_idx == 0;
      let has_empty_line_before = line_idx > 0 && lines[line_idx - 1].trim().is_empty();

      // Detect first-nested: first non-empty content line after `{`
      let is_first_nested = if line_idx > 0 {
        let mut k = line_idx - 1;
        loop {
          let prev = lines[k].trim();
          if prev.is_empty() {
            if k == 0 {
              break false;
            }
            k -= 1;
            continue;
          }
          break prev.ends_with('{');
        }
      } else {
        false
      };

      // Previous non-empty line content
      let prev_non_empty = if line_idx > 0 {
        let mut k = line_idx - 1;
        loop {
          let prev = lines[k].trim();
          if !prev.is_empty() {
            break Some(prev);
          }
          if k == 0 {
            break None;
          }
          k -= 1;
        }
      } else {
        None
      };

      let is_after_comment = prev_non_empty
        .map(|p| p.starts_with("//") || p.starts_with("/*") || p.ends_with("*/"))
        .unwrap_or(false);

      // Check if the previous declaration was a $variable. This includes
      // multi-line declarations like:
      //   $var: some-fn(
      //     arg1,
      //     arg2
      //   ) !default;     <- prev_non_empty would be ") !default;"
      // In that case we need to trace back to the statement start.
      let is_after_dollar_variable = {
        let direct_match = prev_non_empty
          .map(|p| p.starts_with('$') && p.contains(':'))
          .unwrap_or(false);

        if direct_match {
          true
        } else if let Some(prev) = prev_non_empty {
          // Check if the prev line ends with `) !default;` or `)` or `;`
          // indicating the end of a multi-line declaration. Walk back to
          // find the statement's opening line and check if it's a $var.
          let is_multi_line_end = prev.ends_with(") !default;")
            || prev.ends_with(");")
            || (prev.starts_with(')') && (prev.ends_with(';') || prev.ends_with("!default;")));

          if is_multi_line_end && line_idx > 0 {
            // Count unmatched closing parens to find where multi-line
            // statement started. Walk backwards.
            let mut k = line_idx - 1;
            let mut paren_balance: i32 = 0;
            let mut found = false;
            loop {
              let l = lines[k].trim();
              for ch in l.chars().rev() {
                if ch == ')' {
                  paren_balance += 1;
                } else if ch == '(' {
                  paren_balance -= 1;
                }
              }
              if paren_balance <= 0 {
                // This line opened the statement
                found = l.starts_with('$') && l.contains(':');
                break;
              }
              if k == 0 {
                break;
              }
              k -= 1;
            }
            found
          } else {
            false
          }
        } else {
          false
        }
      };

      // Handle ignore options
      if ignore_after_comment && is_after_comment {
        continue;
      }
      if ignore_inside_single_line_block {
        // Simplified: skip if the line contains both `{` and `}`
        if line.contains('{') && line.contains('}') {
          continue;
        }
      }
      if ignore_after_dollar_variable && is_after_dollar_variable {
        continue;
      }

      // Determine expected state
      let mut expect_empty_line = expectation == "always";

      // Handle except options (they flip the expectation)
      if except_first_nested && (is_first_nested || is_first_line) {
        expect_empty_line = !expect_empty_line;
      }
      if except_after_comment && is_after_comment {
        expect_empty_line = !expect_empty_line;
      }
      if except_after_dollar_variable && is_after_dollar_variable {
        expect_empty_line = !expect_empty_line;
      }

      // Calculate byte offset of this line
      let byte_offset: usize = lines[..line_idx]
        .iter()
        .map(|l| l.len() + 1) // +1 for newline
        .sum();

      // Point to the `$` variable, not the start of the line.
      let leading_ws = line.len() - line.trim_start().len();
      if expect_empty_line && !has_empty_line_before && !is_first_line {
        diagnostics.push(
          Diagnostic::new(NAME, "Expected an empty line before $-variable")
            .span(Span::new(byte_offset + leading_ws, trimmed.len())),
        );
      } else if !expect_empty_line && has_empty_line_before {
        diagnostics.push(
          Diagnostic::new(NAME, "Unexpected empty line before $-variable")
            .span(Span::new(byte_offset + leading_ws, trimmed.len())),
        );
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
  fn always_reports_missing_empty_line() {
    let src = ".foo {\n  color: red;\n  $var: 1;\n}";
    let opts = serde_json::json!(["always"]);
    let rule = configured(&ScssDollarVariableEmptyLineBefore, opts.clone());
    let ctx = scss_ctx_with_source(src);
    let d = rule.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected an empty line"));
  }

  #[test]
  fn always_allows_empty_line_before() {
    let src = ".foo {\n  color: red;\n\n  $var: 1;\n}";
    let opts = serde_json::json!(["always"]);
    let rule = configured(&ScssDollarVariableEmptyLineBefore, opts.clone());
    let ctx = scss_ctx_with_source(src);
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn never_reports_empty_line() {
    let src = ".foo {\n  color: red;\n\n  $var: 1;\n}";
    let opts = serde_json::json!(["never"]);
    let rule = configured(&ScssDollarVariableEmptyLineBefore, opts.clone());
    let ctx = scss_ctx_with_source(src);
    let d = rule.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
    assert!(
      d[0]
        .message
        .contains("Unexpected empty line before $-variable")
    );
  }

  #[test]
  fn never_allows_no_empty_line() {
    let src = ".foo {\n  color: red;\n  $var: 1;\n}";
    let opts = serde_json::json!(["never"]);
    let rule = configured(&ScssDollarVariableEmptyLineBefore, opts.clone());
    let ctx = scss_ctx_with_source(src);
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn except_after_dollar_variable() {
    // "always" + except after-dollar-variable => no empty line needed between $vars
    let src = ".foo {\n\n  $var1: 1;\n  $var2: 2;\n}";
    let opts = serde_json::json!(["always", { "except": ["after-dollar-variable"] }]);
    let rule = configured(&ScssDollarVariableEmptyLineBefore, opts.clone());
    let ctx = scss_ctx_with_source(src);
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn except_first_nested() {
    // "always" + except first-nested => no empty line needed for first child
    let src = ".foo {\n  $var: 1;\n}";
    let opts = serde_json::json!(["always", { "except": ["first-nested"] }]);
    let rule = configured(&ScssDollarVariableEmptyLineBefore, opts.clone());
    let ctx = scss_ctx_with_source(src);
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssDollarVariableEmptyLineBefore, None);
    let ctx = ctx_with_source(".foo {\n  color: red;\n  $var: 1;\n}");
    assert!(rule.check_root(&[], &ctx).is_empty());
  }

  #[test]
  fn default_is_always() {
    let rule = configured(&ScssDollarVariableEmptyLineBefore, None);
    // No options => defaults to "always"
    let src = ".foo {\n  color: red;\n  $var: 1;\n}";
    let ctx = scss_ctx_with_source(src);
    let d = rule.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sideways")),
      ["Invalid option value \"sideways\" for rule \"scss/dollar-variable-empty-line-before\""]
    );
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["always", {
          "except": ["first-nested", "after-comment", "after-dollar-variable"],
          "ignore": ["after-comment", "inside-single-line-block", "after-dollar-variable"],
          "disableFix": true
        }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["always", { "ignore": ["first-nested"] }])
      ),
      [
        "Invalid value \"first-nested\" for option \"ignore\" of rule \"scss/dollar-variable-empty-line-before\""
      ]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["always", { "ignoreProperties": ["x"] }])
      ),
      [
        "Invalid option name \"ignoreProperties\" for rule \"scss/dollar-variable-empty-line-before\""
      ]
    );
  }

  #[test]
  fn ignore_after_dollar_variable_skips_consecutive_variables() {
    let source = "a {\n  color: red;\n\n  $a: 1;\n  $b: 2;\n}\n";
    let lint = |options: serde_json::Value| {
      crate::testing::lint(NAME, options, source, gale_css_parser::Syntax::Scss).len()
    };
    assert_eq!(lint(serde_json::json!("always")), 1);
    assert_eq!(
      lint(serde_json::json!(["always", { "ignore": ["after-dollar-variable"] }])),
      0
    );
  }
}
