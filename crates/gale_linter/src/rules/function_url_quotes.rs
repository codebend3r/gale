use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow quotes around `url()` values.
///
/// Equivalent to Stylelint's `function-url-quotes` rule.
///
/// Primary option: `"always"` (default) or `"never"`.
///
/// Secondary options:
///   - `except`: `["empty"]` — invert the primary for empty `url()` calls.
pub struct FunctionUrlQuotes;

impl Rule for FunctionUrlQuotes {
  fn name(&self) -> &'static str {
    "function-url-quotes"
  }

  fn description(&self) -> &'static str {
    "Require or disallow quotes around url() values"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    // Read the primary option: "always" (default) or "never"
    let option = ctx.primary_option_str().unwrap_or("always");

    // Read secondary options
    let except_empty = ctx
      .secondary_options()
      .and_then(|v| v.get("except"))
      .and_then(|v| v.as_array())
      .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("empty")));

    let mut diags = Vec::new();

    // Collect text areas to check based on node type
    match node {
      CssNode::Style(rule) => {
        for decl in &rule.declarations {
          let search_area =
            get_search_area(decl.span.offset, decl.span.length, ctx.source, &decl.value);
          let base_offset =
            if decl.span.length > 0 && decl.span.offset + decl.span.length <= ctx.source.len() {
              decl.span.offset
            } else {
              0
            };
          check_urls(
            search_area,
            base_offset,
            option,
            except_empty,
            ctx,
            self,
            &mut diags,
          );
        }
      }
      CssNode::Declaration(decl) => {
        let search_area =
          get_search_area(decl.span.offset, decl.span.length, ctx.source, &decl.value);
        let base_offset =
          if decl.span.length > 0 && decl.span.offset + decl.span.length <= ctx.source.len() {
            decl.span.offset
          } else {
            0
          };
        check_urls(
          search_area,
          base_offset,
          option,
          except_empty,
          ctx,
          self,
          &mut diags,
        );
      }
      CssNode::AtRule(at_rule) => {
        // Check @import url(...) and other at-rule params
        let name_lower = at_rule.name.to_ascii_lowercase();
        if name_lower == "import" || name_lower == "document" {
          let params = &at_rule.params;
          // Compute the offset of params within the source
          let params_offset = if at_rule.span.length > 0
            && at_rule.span.offset + at_rule.span.length <= ctx.source.len()
          {
            // Try to find params in source after @name
            let at_rule_src =
              &ctx.source[at_rule.span.offset..at_rule.span.offset + at_rule.span.length];
            if let Some(pos) = at_rule_src.find(params.as_str()) {
              at_rule.span.offset + pos
            } else {
              at_rule.span.offset
            }
          } else {
            at_rule.span.offset
          };
          check_urls(
            params,
            params_offset,
            option,
            except_empty,
            ctx,
            self,
            &mut diags,
          );
        }
      }
      _ => {}
    }

    diags
  }
}

fn get_search_area<'a>(
  offset: usize,
  length: usize,
  source: &'a str,
  fallback: &'a str,
) -> &'a str {
  let end = offset + length;
  if length > 0 && end <= source.len() && offset < end {
    &source[offset..end]
  } else {
    fallback
  }
}

fn check_urls(
  search_area: &str,
  base_offset: usize,
  option: &str,
  except_empty: bool,
  ctx: &RuleContext,
  rule: &FunctionUrlQuotes,
  diags: &mut Vec<Diagnostic>,
) {
  if option == "never" {
    for (rel_offset, url_content, quote_len) in find_quoted_urls(search_area) {
      // In "never" mode with except "empty", skip empty URLs
      // (empty quoted URL = url("") should NOT be flagged when except empty)
      // Actually, except: ["empty"] inverts for empty url() only.
      // "never" + except "empty" means: never quote, EXCEPT empty ones should be quoted.
      // So skip flagging empty quoted urls.
      if except_empty && url_content.is_empty() {
        continue;
      }
      // Skip data URIs — Stylelint does not flag data URIs in either mode
      if is_data_uri(&url_content) {
        continue;
      }
      // Skip URLs with SCSS/Less interpolation — Stylelint skips these
      if (ctx.syntax == Syntax::Scss || ctx.syntax == Syntax::Sass)
        && has_scss_interpolation(&url_content)
      {
        continue;
      }
      if ctx.syntax == Syntax::Less && has_less_interpolation(&url_content) {
        continue;
      }
      let abs_offset = base_offset + rel_offset;
      let total_len = url_content.len() + 2 * quote_len;
      diags.push(
        Diagnostic::new(
          rule.name(),
          "Unexpected quotes around \"url\" function argument".to_string(),
        )
        .severity(rule.default_severity())
        .span(Span::new(abs_offset, total_len))
        .fix(Fix::new(
          "Remove quotes from URL",
          vec![Edit::new(Span::new(abs_offset, total_len), &url_content)],
        )),
      );
    }

    // In "never" + except "empty": flag unquoted empty url()
    if except_empty {
      for (rel_offset, _) in find_empty_urls(search_area) {
        let abs_offset = base_offset + rel_offset;
        diags.push(
          Diagnostic::new(
            rule.name(),
            "Expected quotes around \"url\" function argument".to_string(),
          )
          .severity(rule.default_severity())
          .span(Span::new(abs_offset, 0)),
        );
      }
    }
  } else {
    // "always" mode: flag unquoted URLs
    for (rel_offset, url_content) in find_unquoted_urls(search_area, ctx.syntax) {
      // In "always" + except "empty": skip empty url()
      if except_empty && url_content.is_empty() {
        continue;
      }
      let abs_offset = base_offset + rel_offset;
      let quoted = format!("\"{}\"", url_content);
      diags.push(
        Diagnostic::new(
          rule.name(),
          "Expected quotes around \"url\" function argument".to_string(),
        )
        .severity(rule.default_severity())
        .span(Span::new(abs_offset, url_content.len()))
        .fix(Fix::new(
          "Wrap URL in double quotes",
          vec![Edit::new(Span::new(abs_offset, url_content.len()), &quoted)],
        )),
      );
    }

    // In "always" + except "empty": empty url() should NOT have quotes
    if except_empty {
      for (rel_offset, url_content, quote_len) in find_quoted_urls(search_area) {
        if url_content.is_empty() {
          let abs_offset = base_offset + rel_offset;
          let total_len = url_content.len() + 2 * quote_len;
          diags.push(
            Diagnostic::new(
              rule.name(),
              "Unexpected quotes around \"url\" function argument".to_string(),
            )
            .severity(rule.default_severity())
            .span(Span::new(abs_offset, total_len)),
          );
        }
      }
    }
  }
}

/// Check if a URL content is a data URI.
fn is_data_uri(content: &str) -> bool {
  let trimmed = content.trim();
  trimmed.to_ascii_lowercase().starts_with("data:")
}

/// Check if a URL content contains SCSS interpolation.
fn has_scss_interpolation(content: &str) -> bool {
  content.contains("#{")
}

/// Check if a URL content contains Less variable interpolation.
fn has_less_interpolation(content: &str) -> bool {
  content.contains("@{")
}

/// Check if a URL content is a SCSS variable.
fn is_scss_variable(content: &str) -> bool {
  content.trim().starts_with('$')
}

/// Find empty url() calls (no content at all).
/// Returns (byte_offset_of_paren_content, _).
fn find_empty_urls(value: &str) -> Vec<(usize, ())> {
  let mut results = Vec::new();
  let lower = value.to_ascii_lowercase();
  let mut search_from = 0;
  while let Some(pos) = lower[search_from..].find("url(") {
    let abs_pos = search_from + pos;
    if !is_standalone_url(&lower, abs_pos) {
      search_from = abs_pos + 4;
      continue;
    }
    let content_start = abs_pos + 4;
    if content_start < value.len() {
      let rest = &value[content_start..];
      if let Some(close) = rest.find(')') {
        let content = rest[..close].trim();
        if content.is_empty() {
          results.push((content_start, ()));
        }
      }
    }
    search_from = abs_pos + 4;
  }
  results
}

/// Check whether the `url(` match at `abs_pos` is a standalone `url()` call
/// and not part of a longer function name like `static-url(`.
fn is_standalone_url(value: &str, abs_pos: usize) -> bool {
  if abs_pos == 0 {
    return true;
  }
  let prev = value.as_bytes()[abs_pos - 1];
  // If the char before "url(" is alphanumeric, underscore or hyphen,
  // then "url(" is part of a longer identifier (e.g. "static-url(").
  !(prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'-')
}

/// Find quoted URL contents inside `url(...)` calls.
/// Returns (byte_offset_of_quote, inner_content_string, quote_char_len).
fn find_quoted_urls(value: &str) -> Vec<(usize, String, usize)> {
  let mut results = Vec::new();
  let lower = value.to_ascii_lowercase();
  let mut search_from = 0;
  while let Some(pos) = lower[search_from..].find("url(") {
    let abs_pos = search_from + pos;
    // Skip if "url(" is part of a longer function name like "static-url("
    if !is_standalone_url(&lower, abs_pos) {
      search_from = abs_pos + 4;
      continue;
    }
    let content_start = abs_pos + 4; // skip "url("
    if content_start < value.len() {
      let rest = &value[content_start..];
      let first_non_ws = rest.bytes().position(|b| b != b' ' && b != b'\t');
      if let Some(fns) = first_non_ws {
        let first_char = rest.as_bytes()[fns];
        if first_char == b'"' || first_char == b'\'' {
          // Find the closing quote
          let inner_start = fns + 1;
          if let Some(close_quote) = rest[inner_start..].find(first_char as char) {
            let inner = &rest[inner_start..inner_start + close_quote];
            // Offset points to the opening quote
            let quote_abs = content_start + fns;
            results.push((quote_abs, inner.to_string(), 1));
          }
        }
      }
    }
    search_from = abs_pos + 4;
  }
  results
}

/// Find unquoted URL contents inside `url(...)` calls.
/// Returns (byte_offset_of_content, content_string).
/// Skips data URIs, SCSS interpolation, Less interpolation, and SCSS variables.
fn find_unquoted_urls(value: &str, syntax: Syntax) -> Vec<(usize, String)> {
  let mut results = Vec::new();
  let lower = value.to_ascii_lowercase();
  let mut search_from = 0;
  while let Some(pos) = lower[search_from..].find("url(") {
    let abs_pos = search_from + pos;
    if !is_standalone_url(&lower, abs_pos) {
      search_from = abs_pos + 4;
      continue;
    }
    let content_start = abs_pos + 4; // skip "url("
    if content_start < value.len() {
      let rest = &value[content_start..];
      let first_non_ws = rest.bytes().position(|b| b != b' ' && b != b'\t');
      if let Some(fns) = first_non_ws {
        let first_char = rest.as_bytes()[fns];
        if first_char != b'"' && first_char != b'\'' && first_char != b')' {
          // Find the closing paren (handle nested parens)
          let mut depth = 1i32;
          let mut close = None;
          for (j, &byte) in rest.as_bytes().iter().enumerate() {
            if j == 0 {
              continue;
            } // skip the implicit open paren context
            if byte == b'(' {
              depth += 1;
            } else if byte == b')' {
              depth -= 1;
              if depth == 0 {
                close = Some(j);
                break;
              }
            }
          }
          if let Some(close_pos) = close.or_else(|| rest.find(')')) {
            let content = rest[..close_pos].trim();
            if !content.is_empty() {
              // Skip data URIs — Stylelint does not flag data URIs
              if is_data_uri(content) {
                search_from = abs_pos + 4;
                continue;
              }
              // Skip SCSS interpolation
              if (syntax == Syntax::Scss || syntax == Syntax::Sass)
                && has_scss_interpolation(content)
              {
                search_from = abs_pos + 4;
                continue;
              }
              // Skip Less interpolation
              if syntax == Syntax::Less && has_less_interpolation(content) {
                search_from = abs_pos + 4;
                continue;
              }
              // Skip SCSS variables ($var)
              if (syntax == Syntax::Scss || syntax == Syntax::Sass) && is_scss_variable(content) {
                search_from = abs_pos + 4;
                continue;
              }
              let content_abs = content_start + fns;
              results.push((content_abs, content.to_string()));
            }
          }
        }
      }
    }
    search_from = abs_pos + 4;
  }
  results
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn ctx_scss() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn style_with_value(val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "background".to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_unquoted_url() {
    let d = FunctionUrlQuotes.check(&style_with_value("url(foo.png)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].fix.is_some());
  }

  #[test]
  fn allows_quoted_url() {
    let d = FunctionUrlQuotes.check(&style_with_value("url(\"foo.png\")"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_single_quoted_url() {
    let d = FunctionUrlQuotes.check(&style_with_value("url('foo.png')"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_data_uri() {
    let d = FunctionUrlQuotes.check(
      &style_with_value("url(data:image/png;base64,abc123)"),
      &ctx(),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn skips_scss_interpolation() {
    let d = FunctionUrlQuotes.check(&style_with_value("url(#{$var}/foo.png)"), &ctx_scss());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_scss_variable() {
    let d = FunctionUrlQuotes.check(&style_with_value("url($var)"), &ctx_scss());
    assert!(d.is_empty());
  }

  #[test]
  fn checks_at_import_url() {
    use gale_css_parser::AtRule;
    let node = CssNode::AtRule(AtRule {
      name: "import".to_string(),
      params: "url(foo.css)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    });
    let d = FunctionUrlQuotes.check(&node, &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn message_format_matches_stylelint() {
    let d = FunctionUrlQuotes.check(&style_with_value("url(foo.png)"), &ctx());
    assert_eq!(d.len(), 1);
    assert_eq!(
      d[0].message,
      "Expected quotes around \"url\" function argument"
    );
  }
}
