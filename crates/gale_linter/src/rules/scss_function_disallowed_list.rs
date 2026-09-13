use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Specify a list of disallowed SCSS functions.
///
/// Primary option: an array of function name strings.
///
/// ```json
/// ["color.adjust", "color.scale"]
/// ```
///
/// Equivalent to `scss/function-disallowed-list`.
pub struct ScssFunctionDisallowedList;

impl Rule for ScssFunctionDisallowedList {
  fn name(&self) -> &'static str {
    "scss/function-disallowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of disallowed SCSS functions"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let disallowed: Vec<String> = match ctx.options {
      Some(serde_json::Value::Array(arr)) => arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_ascii_lowercase()))
        .collect(),
      _ => return vec![],
    };

    if disallowed.is_empty() {
      return vec![];
    }

    let mut diags = Vec::new();

    match node {
      CssNode::Style(rule) => {
        for decl in &rule.declarations {
          find_disallowed_functions(&decl.value, decl.span.offset, &disallowed, self, &mut diags);
        }
      }
      CssNode::Declaration(decl) => {
        find_disallowed_functions(&decl.value, decl.span.offset, &disallowed, self, &mut diags);
      }
      _ => {}
    }

    diags
  }
}

fn find_disallowed_functions(
  value: &str,
  base_offset: usize,
  disallowed: &[String],
  rule: &ScssFunctionDisallowedList,
  diags: &mut Vec<Diagnostic>,
) {
  let lower = value.to_ascii_lowercase();
  let bytes = lower.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    if bytes[i] == b'(' && i > 0 {
      // Find start of function name (including dots for namespaced functions like color.adjust)
      let end = i;
      let mut start = i;
      while start > 0
        && (bytes[start - 1].is_ascii_alphanumeric()
          || bytes[start - 1] == b'-'
          || bytes[start - 1] == b'_'
          || bytes[start - 1] == b'.')
      {
        start -= 1;
      }
      if start < end {
        let fname = &lower[start..end];
        if disallowed.iter().any(|d| d == fname) {
          let original_name = &value[start..end];
          diags.push(
            Diagnostic::new(
              rule.name(),
              format!("Unexpected disallowed function \"{original_name}\""),
            )
            .severity(rule.default_severity())
            .span(Span::new(base_offset + start, end - start)),
          );
        }
      }
    }
    i += 1;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn scss_ctx_with_options(opts: &serde_json::Value) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: Some(opts),
    }
  }

  fn decl_node(value: &str) -> CssNode {
    CssNode::Declaration(Declaration {
      property: "color".to_string(),
      value: value.to_string(),
      span: ParserSpan::new(0, value.len()),
      important: false,
    })
  }

  fn style_with_value(value: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: value.to_string(),
        span: ParserSpan::new(0, value.len()),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_disallowed_function() {
    let opts = serde_json::json!(["adjust-color"]);
    let d = ScssFunctionDisallowedList.check(
      &decl_node("adjust-color($c, $red: 10)"),
      &scss_ctx_with_options(&opts),
    );
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("adjust-color"));
  }

  #[test]
  fn reports_namespaced_function() {
    let opts = serde_json::json!(["color.adjust"]);
    let d = ScssFunctionDisallowedList.check(
      &decl_node("color.adjust($c, $red: 10)"),
      &scss_ctx_with_options(&opts),
    );
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("color.adjust"));
  }

  #[test]
  fn allows_non_disallowed_function() {
    let opts = serde_json::json!(["adjust-color"]);
    let d = ScssFunctionDisallowedList
      .check(&decl_node("darken($c, 10%)"), &scss_ctx_with_options(&opts));
    assert!(d.is_empty());
  }

  #[test]
  fn allows_no_function() {
    let opts = serde_json::json!(["adjust-color"]);
    let d = ScssFunctionDisallowedList.check(&decl_node("red"), &scss_ctx_with_options(&opts));
    assert!(d.is_empty());
  }

  #[test]
  fn case_insensitive_match() {
    let opts = serde_json::json!(["Adjust-Color"]);
    let d = ScssFunctionDisallowedList.check(
      &decl_node("adjust-color($c, $red: 10)"),
      &scss_ctx_with_options(&opts),
    );
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn works_in_style_rule() {
    let opts = serde_json::json!(["lighten"]);
    let d = ScssFunctionDisallowedList.check(
      &style_with_value("lighten($c, 10%)"),
      &scss_ctx_with_options(&opts),
    );
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn no_options_no_report() {
    let ctx = RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    };
    let d = ScssFunctionDisallowedList.check(&decl_node("lighten($c, 10%)"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let opts = serde_json::json!(["lighten"]);
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let d = ScssFunctionDisallowedList.check(&decl_node("lighten($c, 10%)"), &ctx);
    assert!(d.is_empty());
  }
}
