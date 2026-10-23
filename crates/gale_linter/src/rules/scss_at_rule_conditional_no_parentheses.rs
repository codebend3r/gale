use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-rule-conditional-no-parentheses";

declare_rule! {
  /// Disallow parentheses in conditional `@` rules (`@if`, `@elsif`, `@else if`, `@while`).
  ///
  /// ```scss
  /// // Bad
  /// @if ($condition) { }
  /// @elsif ($condition) { }
  /// @while ($condition) { }
  ///
  /// // Good
  /// @if $condition { }
  /// @elsif $condition { }
  /// @while $condition { }
  /// ```
  ///
  /// Equivalent to `scss/at-rule-conditional-no-parentheses`.
  pub struct ScssAtRuleConditionalNoParentheses => ScssAtRuleConditionalNoParenthesesCheck {
    name: NAME,
    description: "Disallow parentheses in conditional @ rules (if, elsif, while)",
    default_severity: Warning,
  }
}

/// `scss/at-rule-conditional-no-parentheses`, which takes no options.
pub struct ScssAtRuleConditionalNoParenthesesCheck;

impl ScssAtRuleConditionalNoParenthesesCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssAtRuleConditionalNoParenthesesCheck {
  /// Flags parentheses wrapping the condition of `@if`, `@else if`, `@elsif`
  /// or `@while`, where stylelint-scss reports them: the condition, found in
  /// the at-rule's text, with the `if` of `@else if` left out.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      let params = at.params();
      // PostCSS reads `@else if (...)` as `@else` with params `if (...)`.
      let condition = match at.name().to_ascii_lowercase().as_str() {
        "if" | "elsif" | "while" => params,
        "else" => match params.strip_prefix("if") {
          Some(rest) if rest.is_empty() || rest.starts_with([' ', '(']) => rest,
          _ => continue,
        },
        _ => continue,
      };
      // stylelint-scss tests `/^\(.*\)$/`, whose `.` stops at line breaks,
      // so a condition spread over lines is left alone.
      let condition = condition.trim();
      if condition.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
        || !is_wrapped_in_parens(condition)
      {
        continue;
      }
      // stylelint-scss's `word`: the params with their first `if` removed.
      let word = remove_first_if(params);
      diags.push(
        Diagnostic::new(
          NAME,
          "Unexpected () used to surround statements for @-rules",
        )
        .span(at.word_span(&word)),
      );
    }
    diags
  }
}

/// `params.replace(/\bif\b/, "")`.
fn remove_first_if(params: &str) -> String {
  let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
  let bytes = params.as_bytes();
  let found = params.match_indices("if").find(|&(at, _)| {
    (at == 0 || !is_word(bytes[at - 1])) && bytes.get(at + 2).is_none_or(|&b| !is_word(b))
  });
  match found {
    Some((at, _)) => format!("{}{}", &params[..at], &params[at + 2..]),
    None => params.to_string(),
  }
}

/// Check if the string is wrapped in matching outer parentheses.
/// e.g., `($condition)` -> true, `($a) and ($b)` -> false, `$condition` -> false
fn is_wrapped_in_parens(s: &str) -> bool {
  let bytes = s.as_bytes();
  if bytes.is_empty() || bytes[0] != b'(' || bytes[bytes.len() - 1] != b')' {
    return false;
  }

  // Check that the opening paren at index 0 matches the closing paren
  // at the end, not some intermediate one.
  let mut depth = 0;
  for (i, &b) in bytes.iter().enumerate() {
    match b {
      b'(' => depth += 1,
      b')' => {
        depth -= 1;
        if depth == 0 && i != bytes.len() - 1 {
          return false;
        }
      }
      _ => {}
    }
  }
  depth == 0
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  fn check(source: &str, syntax: Syntax) -> Vec<Diagnostic> {
    configured(&ScssAtRuleConditionalNoParentheses, None).check_source(&context(source, syntax))
  }

  /// The reports for `@name params {}` in SCSS.
  fn at_rule(name: &str, params: &str) -> Vec<Diagnostic> {
    check(&format!("@{name} {params} {{ a: b }}"), Syntax::Scss)
  }

  #[test]
  fn reports_parenthesized_if_at_the_paren() {
    let d = at_rule("if", "($condition)");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected ()"));
    assert_eq!((d[0].span.offset, d[0].span.length), (4, 12));
  }

  #[test]
  fn reports_parenthesized_else_if_and_if_in_rules() {
    let scss = "a { @if ($a) { b: c } @else if ($d) { e: f } @else { g: h } }";
    let d = check(scss, Syntax::Scss);
    assert_eq!(d.len(), 2);
    assert_eq!(d[0].span.offset, 8);
    // stylelint-scss's word is ` ($d)`, the params without their `if`.
    assert_eq!((d[1].span.offset, d[1].span.length), (30, 5));
  }

  #[test]
  fn reports_parenthesized_elsif_and_while() {
    assert_eq!(at_rule("elsif", "($x > 1)").len(), 1);
    assert_eq!(at_rule("while", "($i > 0)").len(), 1);
  }

  #[test]
  fn allows_no_parens() {
    assert!(at_rule("if", "$condition").is_empty());
    assert!(at_rule("elsif", "$x > 1").is_empty());
    assert!(at_rule("while", "$i > 0").is_empty());
  }

  #[test]
  fn allows_parens_spread_over_lines() {
    assert!(at_rule("if", "(\n  $a and $b\n)").is_empty());
  }

  #[test]
  fn allows_inner_parens_not_wrapping() {
    // `($a) and ($b)` — not wrapped in a single outer paren pair
    assert!(at_rule("if", "($a) and ($b)").is_empty());
  }

  #[test]
  fn allows_function_call_in_condition() {
    assert!(at_rule("if", "length($list) > 0").is_empty());
  }

  #[test]
  fn ignores_non_conditional_at_rules() {
    assert!(at_rule("media", "(min-width: 768px)").is_empty());
  }

  #[test]
  fn skips_non_scss() {
    assert!(check("@if ($x) { a: b }", Syntax::Css).is_empty());
  }

  #[test]
  fn removes_the_first_whole_word_if() {
    assert_eq!(remove_first_if("if ($a)"), " ($a)");
    assert_eq!(remove_first_if("($gif)"), "($gif)");
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/at-rule-conditional-no-parentheses\""]
    );
  }
}
