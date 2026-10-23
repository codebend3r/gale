use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/load-no-partial-leading-underscore";

declare_rule! {
  /// Disallow leading underscore in partial names within `@use`, `@forward`,
  /// and `@import`.
  pub struct ScssLoadNoPartialLeadingUnderscore => ScssLoadNoPartialLeadingUnderscoreCheck {
    name: NAME,
    description: "Disallow leading underscore in partial names",
    default_severity: Warning,
  }
}

/// `scss/load-no-partial-leading-underscore`, which takes no options.
pub struct ScssLoadNoPartialLeadingUnderscoreCheck;

impl ScssLoadNoPartialLeadingUnderscoreCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssLoadNoPartialLeadingUnderscoreCheck {
  /// Flags a `@use`, `@forward` or `@import` path whose partial name starts with
  /// an underscore; Sass adds it implicitly.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      if !matches!(at.name(), "use" | "forward" | "import") {
        continue;
      }
      let params = at.params();
      // Extract the path from params. Strip quotes and whitespace.
      let path = extract_path(params);
      if path.is_empty() {
        continue;
      }

      // Match Stylelint's behavior: skip if the path-stripped value ends with
      // a whitespace/comma/paren/quote followed by a word (e.g., `as name`
      // after `@use 'path' as name`). Stylelint's imperfect quote stripping
      // causes it to skip these cases.
      if has_trailing_keyword(&strip_quotes(params)) {
        continue;
      }

      // Skipping importing CSS: url(), ".css", URI with a protocol
      if path.starts_with("url(") || path.ends_with(".css") || path.contains("//") {
        continue;
      }

      // Get the filename portion (last segment after `/`).
      let filename = path.rsplit('/').next().unwrap_or(path);
      if !filename.starts_with('_') {
        continue;
      }
      // Point at the path in the at-rule, matching Stylelint's column
      // positioning (word: pathStripped).
      diags.push(
        Diagnostic::new(
          NAME,
          "Unexpected leading underscore in imported partial name",
        )
        .span(at.word_span(path)),
      );
    }
    diags
  }
}

/// Strip leading and trailing quotes from a string (one level only),
/// matching Stylelint's approach: strip leading `["']\s*` and trailing `\s*["']`.
fn strip_quotes(params: &str) -> String {
  let s = params.trim();
  let s = if s.starts_with('"') || s.starts_with('\'') {
    &s[1..]
  } else {
    s
  };
  // Strip trailing quote (find last quote)
  if let Some(pos) = s.rfind(['"', '\'']) {
    s[..pos].to_string() + &s[pos + 1..]
  } else {
    s.to_string()
  }
}

/// Check if a path-stripped string ends with a whitespace/comma/paren/quote
/// followed by word characters (e.g., ` as theme`).
/// This matches Stylelint's regex: `/[\s,)"']\w+$/`
fn has_trailing_keyword(s: &str) -> bool {
  let bytes = s.as_bytes();
  let len = bytes.len();
  if len == 0 {
    return false;
  }
  // Find the last word boundary: scan backwards for word chars
  let mut end = len;
  while end > 0 && bytes[end - 1].is_ascii_alphanumeric() || (end > 0 && bytes[end - 1] == b'_') {
    end -= 1;
  }
  if end == len || end == 0 {
    return false;
  }
  let prev = bytes[end - 1];
  matches!(
    prev,
    b' ' | b'\t' | b'\n' | b'\r' | b',' | b')' | b'"' | b'\''
  )
}

/// Extract a path string from at-rule params, stripping quotes.
/// Handles `@use 'path' as alias` by finding the content between quote chars.
fn extract_path(params: &str) -> &str {
  let trimmed = params.trim();
  // Quoted path: find content between matching quotes, ignoring any trailing
  // ` as <alias>` or other keywords that follow the closing quote.
  if let Some(s) = trimmed.strip_prefix('"') {
    if let Some(end) = s.find('"') {
      return &s[..end];
    }
  }
  if let Some(s) = trimmed.strip_prefix('\'') {
    if let Some(end) = s.find('\'') {
      return &s[..end];
    }
  }
  // Unquoted: take first whitespace-separated token.
  trimmed.split_whitespace().next().unwrap_or(trimmed)
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  fn check(source: &str, syntax: Syntax) -> Vec<Diagnostic> {
    configured(&ScssLoadNoPartialLeadingUnderscore, None).check_source(&context(source, syntax))
  }

  fn use_rule(params: &str) -> Vec<Diagnostic> {
    check(&format!("@use {params};"), Syntax::Scss)
  }

  #[test]
  fn reports_leading_underscore_in_use_at_the_path() {
    let d = use_rule("\"_variables\"");
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].span.offset, d[0].span.length), (6, 10));
  }

  #[test]
  fn reports_leading_underscore_in_import_and_forward() {
    assert_eq!(check("@import \"_mixins\";", Syntax::Scss).len(), 1);
    assert_eq!(check("@forward '_mixins';", Syntax::Scss).len(), 1);
  }

  #[test]
  fn reports_underscore_in_path() {
    assert_eq!(use_rule("\"path/to/_file\"").len(), 1);
  }

  #[test]
  fn allows_no_underscore_and_aliases() {
    assert!(use_rule("\"variables\"").is_empty());
    assert!(use_rule("\"path/to/file\"").is_empty());
    assert!(use_rule("\"_theme\" as theme").is_empty());
  }

  #[test]
  fn skips_non_scss() {
    assert!(check("@use \"_variables\";", Syntax::Css).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/load-no-partial-leading-underscore\""]
    );
  }
}
