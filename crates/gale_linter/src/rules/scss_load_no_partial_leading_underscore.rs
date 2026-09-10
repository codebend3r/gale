use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow leading underscore in partial names within `@use`, `@forward`,
/// and `@import`.
pub struct ScssLoadNoPartialLeadingUnderscore;

impl Rule for ScssLoadNoPartialLeadingUnderscore {
  fn name(&self) -> &'static str {
    "scss/load-no-partial-leading-underscore"
  }

  fn description(&self) -> &'static str {
    "Disallow leading underscore in partial names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if !matches!(at.name.as_str(), "use" | "forward" | "import") {
      return vec![];
    }

    // Extract the path from params. Strip quotes and whitespace.
    let path = extract_path(&at.params);
    if path.is_empty() {
      return vec![];
    }

    // Match Stylelint's behavior: skip if the path-stripped value ends with
    // a whitespace/comma/paren/quote followed by a word (e.g., `as name`
    // after `@use 'path' as name`). Stylelint's imperfect quote stripping
    // causes it to skip these cases.
    let path_stripped = strip_quotes(&at.params);
    if has_trailing_keyword(&path_stripped) {
      return vec![];
    }

    // Skipping importing CSS: url(), ".css", URI with a protocol
    if path.starts_with("url(") || path.ends_with(".css") || path.contains("//") {
      return vec![];
    }

    // Get the filename portion (last segment after `/`).
    let filename = path.rsplit('/').next().unwrap_or(path);
    if filename.starts_with('_') {
      // Compute the span pointing to the path within the params,
      // matching Stylelint's column positioning (word: pathStripped).
      let path_offset = if at.span.length > 0 && at.span.offset + at.span.length <= ctx.source.len()
      {
        let at_src = &ctx.source[at.span.offset..at.span.offset + at.span.length];
        // Find the path string within the at-rule source
        if let Some(pos) = at_src.find(path) {
          at.span.offset + pos
        } else {
          at.span.offset
        }
      } else {
        at.span.offset
      };
      vec![
        Diagnostic::new(
          self.name(),
          "Unexpected leading underscore in imported partial name".to_string(),
        )
        .severity(self.default_severity())
        .span(Span::new(path_offset, path.len())),
      ]
    } else {
      vec![]
    }
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
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn use_rule(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "use".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    })
  }

  fn import_rule(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "import".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    })
  }

  #[test]
  fn reports_leading_underscore_in_use() {
    let d = ScssLoadNoPartialLeadingUnderscore.check(&use_rule("\"_variables\""), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_leading_underscore_in_import() {
    let d = ScssLoadNoPartialLeadingUnderscore.check(&import_rule("\"_mixins\""), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_underscore_in_path() {
    let d = ScssLoadNoPartialLeadingUnderscore.check(&use_rule("\"path/to/_file\""), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_no_underscore() {
    let d = ScssLoadNoPartialLeadingUnderscore.check(&use_rule("\"variables\""), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_path_without_underscore() {
    let d = ScssLoadNoPartialLeadingUnderscore.check(&use_rule("\"path/to/file\""), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    assert!(
      ScssLoadNoPartialLeadingUnderscore
        .check(&use_rule("\"_variables\""), &ctx)
        .is_empty()
    );
  }
}
