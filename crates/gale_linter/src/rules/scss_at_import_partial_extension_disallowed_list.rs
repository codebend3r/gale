use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Specify a list of disallowed file extensions for partial names in
/// `@import`, `@use`, and `@forward` commands.
///
/// Primary option: array of strings (e.g. `[".scss", ".sass"]`).
///
/// Equivalent to `scss/at-import-partial-extension-disallowed-list`.
pub struct ScssAtImportPartialExtensionDisallowedList;

impl Rule for ScssAtImportPartialExtensionDisallowedList {
  fn name(&self) -> &'static str {
    "scss/at-import-partial-extension-disallowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of disallowed file extensions for partial names in @import/@use/@forward"
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

    // Only applies to @import, @use, @forward
    if at.name != "import" && at.name != "use" && at.name != "forward" {
      return vec![];
    }

    let disallowed: Vec<String> = ctx
      .primary_option()
      .and_then(|v| {
        v.as_array().map(|arr| {
          arr
            .iter()
            .filter_map(|s| s.as_str().map(|s| s.to_string()))
            .collect()
        })
      })
      .unwrap_or_default();

    if disallowed.is_empty() {
      return vec![];
    }

    let mut diags = Vec::new();

    for part in at.params.split(',') {
      let path = part.trim().trim_matches('"').trim_matches('\'');

      // Skip URLs and CSS imports
      if path.starts_with("http://")
        || path.starts_with("https://")
        || path.starts_with("//")
        || path.ends_with(".css")
      {
        continue;
      }

      for ext in &disallowed {
        if path.ends_with(ext.as_str()) {
          diags.push(
            Diagnostic::new(
              self.name(),
              format!(
                "Unexpected extension \"{}\" in @{} \"{}\"",
                ext, at.name, path
              ),
            )
            .severity(self.default_severity())
            .span(Span::new(at.span.offset, at.span.length)),
          );
          break;
        }
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn scss_ctx_with_option(opt: &serde_json::Value) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: Some(opt),
    }
  }

  fn at_rule(name: &str, params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: name.to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 20),
      children: vec![],
    })
  }

  #[test]
  fn reports_disallowed_extension_in_import() {
    // Wrap in outer array so primary_option() returns the inner array
    let opt = serde_json::json!([[".scss", ".sass"]]);
    let ctx = scss_ctx_with_option(&opt);
    let d =
      ScssAtImportPartialExtensionDisallowedList.check(&at_rule("import", "\"foo.scss\""), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains(".scss"));
  }

  #[test]
  fn allows_import_without_disallowed_extension() {
    let opt = serde_json::json!([[".scss"]]);
    let ctx = scss_ctx_with_option(&opt);
    let d = ScssAtImportPartialExtensionDisallowedList.check(&at_rule("import", "\"foo\""), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_disallowed_extension_in_use() {
    let opt = serde_json::json!([[".scss"]]);
    let ctx = scss_ctx_with_option(&opt);
    let d = ScssAtImportPartialExtensionDisallowedList.check(&at_rule("use", "\"bar.scss\""), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@use"));
  }

  #[test]
  fn reports_disallowed_extension_in_forward() {
    let opt = serde_json::json!([[".sass"]]);
    let ctx = scss_ctx_with_option(&opt);
    let d =
      ScssAtImportPartialExtensionDisallowedList.check(&at_rule("forward", "\"baz.sass\""), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@forward"));
  }

  #[test]
  fn allows_css_import() {
    let opt = serde_json::json!([[".scss"]]);
    let ctx = scss_ctx_with_option(&opt);
    let d =
      ScssAtImportPartialExtensionDisallowedList.check(&at_rule("import", "\"foo.css\""), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let opt = serde_json::json!([[".scss"]]);
    let css_ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opt),
    };
    let d = ScssAtImportPartialExtensionDisallowedList
      .check(&at_rule("import", "\"foo.scss\""), &css_ctx);
    assert!(d.is_empty());
  }
}
