use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space before the operator within attribute selectors.
///
/// Equivalent to `@stylistic/selector-attribute-operator-space-before`.
pub struct StylisticSelectorAttributeOperatorSpaceBefore;

/// CSS attribute selector operators.
const ATTR_OPERATORS: &[&str] = &["~=", "|=", "^=", "$=", "*=", "="];

impl Rule for StylisticSelectorAttributeOperatorSpaceBefore {
  fn name(&self) -> &'static str {
    "@stylistic/selector-attribute-operator-space-before"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space before the operator within attribute selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let option = ctx.primary_option_str().unwrap_or("never");
    let selector = &rule.selector;

    let mut diags = Vec::new();
    let sel_offset = rule.span.offset;

    // Find attribute selectors [...]
    let bytes = selector.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
      if bytes[i] == b'[' {
        let bracket_start = i;
        // Find the closing bracket
        let mut depth = 1;
        i += 1;
        while i < len && depth > 0 {
          if bytes[i] == b'[' {
            depth += 1;
          } else if bytes[i] == b']' {
            depth -= 1;
          }
          if depth > 0 {
            i += 1;
          }
        }
        let bracket_end = i;
        if bracket_end < len {
          let attr_content = &selector[bracket_start + 1..bracket_end];
          check_attr_operator(
            attr_content,
            sel_offset + bracket_start + 1,
            option,
            self,
            &mut diags,
          );
        }
      }
      i += 1;
    }

    diags
  }
}

fn check_attr_operator(
  attr_content: &str,
  base_offset: usize,
  option: &str,
  rule: &StylisticSelectorAttributeOperatorSpaceBefore,
  diags: &mut Vec<Diagnostic>,
) {
  for op in ATTR_OPERATORS {
    if let Some(pos) = attr_content.find(op) {
      if pos == 0 {
        continue;
      }
      let char_before = attr_content.as_bytes()[pos - 1];
      let has_space = char_before == b' ' || char_before == b'\t';

      let violation = match option {
        "always" => !has_space,
        "never" => has_space,
        _ => false,
      };

      if violation {
        let msg = match option {
          "always" => format!("Expected single space before \"{op}\""),
          "never" => format!("Unexpected whitespace before \"{op}\""),
          _ => return,
        };
        diags.push(
          Diagnostic::new(rule.name(), msg)
            .severity(rule.default_severity())
            .span(Span::new(base_offset + pos, op.len())),
        );
      }
      return; // Only check the first operator found
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_selector(sel: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: vec![],
      span: ParserSpan::new(0, sel.len()),
      ..Default::default()
    })
  }

  #[test]
  fn reports_space_before_operator_when_never() {
    let rule = StylisticSelectorAttributeOperatorSpaceBefore;
    let d = rule.check(&style_with_selector("[attr =value]"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected whitespace"));
  }

  #[test]
  fn allows_no_space_before_operator_when_never() {
    let rule = StylisticSelectorAttributeOperatorSpaceBefore;
    let d = rule.check(&style_with_selector("[attr=value]"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn no_report_for_simple_attribute() {
    let rule = StylisticSelectorAttributeOperatorSpaceBefore;
    let d = rule.check(&style_with_selector("[disabled]"), &ctx());
    assert!(d.is_empty());
  }
}
