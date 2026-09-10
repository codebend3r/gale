use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Only allow specified URL schemes inside `url()` functions.
///
/// Options: an array of allowed scheme names (e.g., `["https", "data"]`).
/// Scheme-relative URLs (starting with `//`) and relative URLs (no scheme)
/// are always allowed.
///
/// Equivalent to Stylelint's `function-url-scheme-allowed-list` rule.
pub struct FunctionUrlSchemeAllowedList;

impl Rule for FunctionUrlSchemeAllowedList {
  fn name(&self) -> &'static str {
    "function-url-scheme-allowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of allowed URL schemes"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let allowed: Vec<String> = match ctx.options {
      Some(serde_json::Value::Array(arr)) => arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_ascii_lowercase()))
        .collect(),
      _ => return vec![],
    };

    let mut diags = Vec::new();

    let declarations: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    for decl in declarations {
      check_url_schemes(&decl.value, decl.span.offset, &allowed, self, &mut diags);
    }

    diags
  }
}

fn check_url_schemes(
  value: &str,
  base_offset: usize,
  allowed: &[String],
  rule: &FunctionUrlSchemeAllowedList,
  diags: &mut Vec<Diagnostic>,
) {
  let lower = value.to_ascii_lowercase();

  // Find all url(...) occurrences
  let mut search_from = 0;
  while let Some(url_start) = lower[search_from..].find("url(") {
    let abs_start = search_from + url_start;
    let after_paren = abs_start + 4;
    if after_paren >= lower.len() {
      break;
    }

    // Find the closing paren
    let content_start = after_paren;
    let content_end = match lower[content_start..].find(')') {
      Some(pos) => content_start + pos,
      None => break,
    };

    let mut content = lower[content_start..content_end].trim();

    // Strip quotes
    if (content.starts_with('"') && content.ends_with('"'))
      || (content.starts_with('\'') && content.ends_with('\''))
    {
      content = &content[1..content.len() - 1];
    }

    // Skip scheme-relative (//...) and relative URLs (no scheme)
    if !content.starts_with("//")
      && let Some(scheme_end) = content.find("://")
    {
      let scheme = &content[..scheme_end];
      if !allowed.contains(&scheme.to_string()) {
        diags.push(
          Diagnostic::new(rule.name(), format!("Unexpected URL scheme \"{scheme}\""))
            .severity(rule.default_severity())
            .span(Span::new(
              base_offset + abs_start,
              content_end - abs_start + 1,
            )),
        );
      }
    }

    search_from = content_end + 1;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx_with_options(options: Option<serde_json::Value>) -> RuleContext<'static> {
    let opts: Option<&'static serde_json::Value> = options.map(|v| &*Box::leak(Box::new(v)));
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: opts,
    }
  }

  fn style_with_value(val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "background".to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, val.len()),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn allows_all_when_no_options() {
    let ctx = ctx_with_options(None);
    let d = FunctionUrlSchemeAllowedList.check(&style_with_value("url(http://example.com)"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_listed_scheme() {
    let ctx = ctx_with_options(Some(serde_json::json!(["https", "data"])));
    let d = FunctionUrlSchemeAllowedList
      .check(&style_with_value("url(https://example.com/img.png)"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn flags_unlisted_scheme() {
    let ctx = ctx_with_options(Some(serde_json::json!(["https"])));
    let d = FunctionUrlSchemeAllowedList
      .check(&style_with_value("url(http://example.com/img.png)"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("http"));
  }

  #[test]
  fn allows_relative_urls() {
    let ctx = ctx_with_options(Some(serde_json::json!(["https"])));
    let d = FunctionUrlSchemeAllowedList.check(&style_with_value("url(images/bg.png)"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_scheme_relative_urls() {
    let ctx = ctx_with_options(Some(serde_json::json!(["https"])));
    let d =
      FunctionUrlSchemeAllowedList.check(&style_with_value("url(//example.com/img.png)"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(
      FunctionUrlSchemeAllowedList.name(),
      "function-url-scheme-allowed-list"
    );
  }
}
