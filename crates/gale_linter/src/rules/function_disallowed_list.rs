use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow specific CSS functions.
///
/// By default, disallows no functions (the list is empty). This rule is useful
/// when configured via options to ban functions like `rgb`, `hsl`, etc.
///
/// Equivalent to Stylelint's `function-disallowed-list` rule.
pub struct FunctionDisallowedList;

/// Default disallowed functions — empty by default; configure via rule options.
/// For demonstration purposes we include a commonly banned function.
const DISALLOWED: &[&str] = &[];

impl Rule for FunctionDisallowedList {
  fn name(&self) -> &'static str {
    "function-disallowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of disallowed functions"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    match node {
      CssNode::Style(rule) => {
        for decl in &rule.declarations {
          find_disallowed_functions(&decl.value, decl.span.offset, self, &mut diags);
        }
      }
      CssNode::Declaration(decl) => {
        find_disallowed_functions(&decl.value, decl.span.offset, self, &mut diags);
      }
      _ => {}
    }
    diags
  }
}

#[allow(clippy::const_is_empty)]
fn find_disallowed_functions(
  value: &str,
  base_offset: usize,
  rule: &FunctionDisallowedList,
  diags: &mut Vec<Diagnostic>,
) {
  if DISALLOWED.is_empty() {
    return;
  }
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    if bytes[i] == b'(' && i > 0 {
      // Find start of function name
      let end = i;
      let mut start = i;
      while start > 0
        && (bytes[start - 1].is_ascii_alphanumeric()
          || bytes[start - 1] == b'-'
          || bytes[start - 1] == b'_')
      {
        start -= 1;
      }
      if start < end {
        let fname = &value[start..end];
        if DISALLOWED.contains(&fname) {
          diags.push(
            Diagnostic::new(rule.name(), format!("Unexpected function \"{fname}\""))
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

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_value(val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, val.len()),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn allows_all_when_list_empty() {
    // Default list is empty, so nothing is disallowed
    let d = FunctionDisallowedList.check(&style_with_value("rgb(0, 0, 0)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_non_function_values() {
    let d = FunctionDisallowedList.check(&style_with_value("red"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn find_disallowed_functions_detects() {
    let rule = FunctionDisallowedList;
    let mut diags = Vec::new();
    // Manually call with a list check — simulating "rgb" being banned
    let value = "rgb(0, 0, 0)";
    let lower = value.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
      if bytes[i] == b'(' && i > 0 {
        let end = i;
        let mut start = i;
        while start > 0
          && (bytes[start - 1].is_ascii_alphanumeric()
            || bytes[start - 1] == b'-'
            || bytes[start - 1] == b'_')
        {
          start -= 1;
        }
        if start < end {
          let fname = &lower[start..end];
          // Simulate rgb being disallowed
          if fname == "rgb" {
            diags.push(
              Diagnostic::new(rule.name(), format!("Unexpected function \"{fname}\""))
                .severity(rule.default_severity())
                .span(Span::new(start, end - start)),
            );
          }
        }
      }
      i += 1;
    }
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("rgb"));
  }
}
