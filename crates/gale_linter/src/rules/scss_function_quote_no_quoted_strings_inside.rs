use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/function-quote-no-quoted-strings-inside";

declare_rule! {
  /// Disallow quoted strings inside `quote()`.
  pub struct ScssFunctionQuoteNoQuotedStringsInside => ScssFunctionQuoteNoQuotedStringsInsideCheck {
    name: NAME,
    description: "Disallow quoted strings inside quote()",
    default_severity: Warning,
  }
}

/// `scss/function-quote-no-quoted-strings-inside`, which takes no options.
pub struct ScssFunctionQuoteNoQuotedStringsInsideCheck;

impl ScssFunctionQuoteNoQuotedStringsInsideCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssFunctionQuoteNoQuotedStringsInsideCheck {
  /// Flags `quote()` calls whose argument is already a quoted string.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    tree
      .walk_decls()
      .filter(|decl| has_quoted_argument(decl.written_value()))
      .map(|decl| {
        Diagnostic::new(NAME, "Unexpected quoted string inside quote()").span(decl.span())
      })
      .collect()
  }
}

/// Whether a `quote(` in `value` (as written) takes a quoted string.
fn has_quoted_argument(value: &str) -> bool {
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    // Look for "quote(" but ensure it's not part of another function name
    // like "unquote(" or "str-quote("
    let Some(pos) = value[i..].find("quote(") else {
      break;
    };
    let abs_pos = i + pos;
    // Check that the character before "quote(" is not alphanumeric, hyphen, or underscore
    let is_standalone = abs_pos == 0 || {
      let prev = bytes[abs_pos - 1];
      !prev.is_ascii_alphanumeric() && prev != b'-' && prev != b'_'
    };

    if is_standalone {
      let trimmed = value[abs_pos + 6..].trim_start();
      if trimmed.starts_with('"') || trimmed.starts_with('\'') {
        return true;
      }
    }

    i = abs_pos + 6;
  }

  false
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, invalid_options, scss_ctx_with_source};

  /// A top-level `$content` variable set to `value`.
  fn decl(value: &str) -> String {
    format!("$content: {value};")
  }

  #[test]
  fn reports_quoted_string_inside_quote() {
    let rule = configured(&ScssFunctionQuoteNoQuotedStringsInside, None);
    let d = rule.check_source(&scss_ctx_with_source(&decl("quote(\"hello\")")));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_unquoted_inside_quote() {
    let rule = configured(&ScssFunctionQuoteNoQuotedStringsInside, None);
    let d = rule.check_source(&scss_ctx_with_source(&decl("quote($var)")));
    assert!(d.is_empty());
  }

  #[test]
  fn allows_unquote_with_quoted_string() {
    let rule = configured(&ScssFunctionQuoteNoQuotedStringsInside, None);
    // `unquote("hello")` should NOT be flagged — it's unquote, not quote
    let d = rule.check_source(&scss_ctx_with_source(&decl("unquote(\"hello\")")));
    assert!(d.is_empty());
  }

  #[test]
  fn allows_str_quote_variant() {
    let rule = configured(&ScssFunctionQuoteNoQuotedStringsInside, None);
    // Function names ending in `-quote` should not be flagged
    let d = rule.check_source(&scss_ctx_with_source(&decl("str-quote(\"hello\")")));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_standalone_quote_with_quoted() {
    let rule = configured(&ScssFunctionQuoteNoQuotedStringsInside, None);
    // Standalone `quote("hello")` should still be flagged
    let d = rule.check_source(&scss_ctx_with_source(&decl("quote(\"hello\")")));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_quote_after_space() {
    let rule = configured(&ScssFunctionQuoteNoQuotedStringsInside, None);
    // `foo quote("hello")` — quote after space is standalone
    let d = rule.check_source(&scss_ctx_with_source(&decl("foo quote(\"hello\")")));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn looks_at_every_declaration() {
    let rule = configured(&ScssFunctionQuoteNoQuotedStringsInside, None);
    let source = "@mixin m { content: quote(\"x\"); }\na { content: quote(\"x\"); }";
    let d = rule.check_source(&scss_ctx_with_source(source));
    let spans: Vec<(usize, usize)> = d.iter().map(|d| (d.span.offset, d.span.length)).collect();
    assert_eq!(spans, [(11, 20), (38, 20)]);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      [
        "Unexpected option value \"always\" for rule \"scss/function-quote-no-quoted-strings-inside\""
      ]
    );
  }
}
