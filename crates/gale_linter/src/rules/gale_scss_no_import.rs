use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow the Sass `@import` rule, which Dart Sass deprecated in 1.80.0
/// and removes in 3.0.0.
///
/// Gale's own rule; Stylelint and stylelint-scss have no equivalent.
///
/// Only a Sass import is reported.  The imports Sass passes through as plain
/// CSS, and keeps after 3.0.0, are left alone: a URL that ends in `.css`,
/// starts with `http://`, `https://` or `//`, is written as `url()` or
/// contains interpolation, and an import with media queries or any other
/// condition after the URL.
///
/// ```scss
/// @import "variables";      // reported: use @use or @forward
/// @import "theme.css";      // plain CSS, allowed
/// @import "print" print;    // plain CSS, allowed
/// ```
pub struct GaleScssNoImport;

/// Splits `params` on the commas between imports, ignoring commas inside
/// strings and parentheses.
fn split_imports(params: &str) -> Vec<&str> {
  let mut parts = Vec::new();
  let mut depth = 0usize;
  let mut quote: Option<char> = None;
  let mut start = 0;
  let mut escaped = false;
  for (i, c) in params.char_indices() {
    if escaped {
      escaped = false;
      continue;
    }
    match (quote, c) {
      (_, '\\') => escaped = true,
      (Some(q), c) if c == q => quote = None,
      (Some(_), _) => {}
      (None, '"' | '\'') => quote = Some(c),
      (None, '(') => depth += 1,
      (None, ')') => depth = depth.saturating_sub(1),
      (None, ',') if depth == 0 => {
        parts.push(&params[start..i]);
        start = i + 1;
      }
      _ => {}
    }
  }
  parts.push(&params[start..]);
  parts
}

/// Whether one import in an `@import` list is one Sass passes through as
/// plain CSS rather than loading itself.
fn is_plain_css_import(import: &str) -> bool {
  let import = import.trim();
  if import.is_empty() || import.contains("#{") {
    return true;
  }
  if import.len() >= 4 && import[..4].eq_ignore_ascii_case("url(") {
    return true;
  }

  let (url, rest) = match import.chars().next() {
    Some(q @ ('"' | '\'')) => match import[1..].find(q) {
      Some(end) => (&import[1..1 + end], &import[2 + end..]),
      None => (&import[1..], ""),
    },
    _ => match import.find(char::is_whitespace) {
      Some(end) => (&import[..end], &import[end..]),
      None => (import, ""),
    },
  };

  !rest.trim().is_empty()
    || url.to_ascii_lowercase().ends_with(".css")
    || url.starts_with("http://")
    || url.starts_with("https://")
    || url.starts_with("//")
}

impl Rule for GaleScssNoImport {
  fn name(&self) -> &'static str {
    "gale/scss-no-import"
  }

  fn description(&self) -> &'static str {
    "Disallow Sass @import, which Dart Sass 3 removes"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports an `@import` that loads at least one Sass file.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    let CssNode::AtRule(at) = node else {
      return vec![];
    };
    if !at.name.eq_ignore_ascii_case("import") {
      return vec![];
    }
    if split_imports(&at.params)
      .into_iter()
      .all(is_plain_css_import)
    {
      return vec![];
    }

    vec![
      Diagnostic::new(self.name(), "Expected @use or @forward instead of @import")
        .severity(self.default_severity())
        .span(Span::new(at.span.offset, at.span.length)),
    ]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan};

  use crate::testing::{ctx, scss_ctx};

  fn import(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "import".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 20),
      children: vec![],
    })
  }

  fn reports(params: &str) -> bool {
    !GaleScssNoImport
      .check(&import(params), &scss_ctx())
      .is_empty()
  }

  #[test]
  fn reports_a_sass_import() {
    let diags = GaleScssNoImport.check(&import("\"variables\""), &scss_ctx());
    assert_eq!(diags.len(), 1);
    assert_eq!(
      diags[0].message,
      "Expected @use or @forward instead of @import"
    );
  }

  #[test]
  fn reports_a_list_once_when_any_import_is_sass() {
    assert!(reports("\"mixins\", \"functions\""));
    assert!(reports("\"theme.css\", \"mixins\""));
    assert_eq!(
      GaleScssNoImport
        .check(&import("\"a\", \"b\""), &scss_ctx())
        .len(),
      1
    );
  }

  #[test]
  fn reports_partials_and_sass_extensions() {
    assert!(reports("'_partial'"));
    assert!(reports("\"theme.scss\""));
    assert!(reports("foo"));
  }

  #[test]
  fn allows_plain_css_imports() {
    for params in [
      "\"theme.css\"",
      "\"THEME.CSS\"",
      "url(\"fonts.css\")",
      "url(fonts)",
      "\"https://example.com/reset\"",
      "\"http://example.com/reset\"",
      "\"//cdn.example.com/reset\"",
      "\"print\" print",
      "\"layout\" supports(display: grid)",
      "\"layout\" layer(base)",
      "\"theme-#{$name}\"",
      "\"a.css\", url(b, c)",
    ] {
      assert!(!reports(params), "{params} should be plain CSS");
    }
  }

  #[test]
  fn skips_css_files() {
    assert!(
      GaleScssNoImport
        .check(&import("\"variables\""), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn ignores_other_at_rules() {
    let node = CssNode::AtRule(AtRule {
      name: "use".to_string(),
      params: "\"variables\"".to_string(),
      span: ParserSpan::new(0, 16),
      children: vec![],
    });
    assert!(GaleScssNoImport.check(&node, &scss_ctx()).is_empty());
  }
}
