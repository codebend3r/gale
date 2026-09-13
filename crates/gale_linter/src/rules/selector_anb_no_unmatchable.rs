use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow unmatchable An+B selectors like `:nth-child(0n+0)` or
/// `:nth-child(-n+0)` which can never match any element.
///
/// Equivalent to Stylelint's `selector-anb-no-unmatchable` rule.
pub struct SelectorAnbNoUnmatchable;

/// The pseudo-class functions that take An+B notation.
const ANB_PSEUDOS: &[&str] = &[
  ":nth-child(",
  ":nth-last-child(",
  ":nth-of-type(",
  ":nth-last-of-type(",
];

/// Parse an An+B expression and return whether it is unmatchable.
///
/// An An+B expression generates the sequence: A*0+B, A*1+B, A*2+B, ...
/// for non-negative integer indices. An element is matched if any value in
/// the sequence is a positive integer (>= 1).
///
/// Unmatchable cases:
/// - `0` (or `0n+0`) — the sequence is just {0}, never >= 1
/// - `-n+0` — sequence is 0, -1, -2, ... (A=-1, B=0)
/// - Any expression where A <= 0 and B <= 0
fn is_unmatchable(expr: &str) -> bool {
  let expr = expr.trim().to_ascii_lowercase();

  // Named keywords are always matchable
  if expr == "odd" || expr == "even" || expr == "n" {
    return false;
  }

  // Parse An+B
  let (a, b) = parse_anb(&expr);

  if a == 0 {
    // Pure offset: matches element at position B
    b <= 0
  } else if a > 0 {
    // Positive step: eventually reaches any positive number if B could be reached
    // Sequence: B, A+B, 2A+B, ... — matchable if any term >= 1
    // The first term is B itself; if B >= 1, matchable.
    // If B < 1, check if A*k + B >= 1 for some k >= 0, i.e. k >= (1-B)/A
    // Since A > 0, this always has a solution. So positive A is always matchable
    // unless B is so negative that we never reach 1 — but with positive step we
    // always will. Actually: the smallest value is B (at k=0). If B >= 1, done.
    // If B < 1, at k=1 we get A+B. E.g. A=2, B=-5 => -5, -3, -1, 1, 3 => matchable.
    // So positive A is always matchable.
    false
  } else {
    // Negative step (A < 0): sequence is B, A+B, 2A+B, ...
    // The maximum value is B (at k=0), then it decreases.
    // Matchable only if B >= 1.
    b <= 0
  }
}

/// Parse an An+B expression into (A, B) coefficients.
fn parse_anb(expr: &str) -> (i64, i64) {
  let expr = expr.replace(' ', "");

  // Just a number (no 'n')
  if !expr.contains('n') {
    return (0, expr.parse::<i64>().unwrap_or(0));
  }

  // Split on 'n'
  let parts: Vec<&str> = expr.splitn(2, 'n').collect();
  let a_str = parts[0];
  let rest = if parts.len() > 1 { parts[1] } else { "" };

  let a = match a_str {
    "" | "+" => 1,
    "-" => -1,
    _ => a_str.parse::<i64>().unwrap_or(0),
  };

  // Rest should be like "+3", "-2", or empty
  let b = if rest.is_empty() {
    0
  } else {
    // rest starts with '+' or '-' followed by digits
    rest.parse::<i64>().unwrap_or(0)
  };

  (a, b)
}

/// Extract An+B expressions from nth-* pseudo-class functions in a selector.
fn extract_anb_expressions(selector: &str) -> Vec<String> {
  let mut exprs = Vec::new();
  let lower = selector.to_ascii_lowercase();

  for pseudo in ANB_PSEUDOS {
    let mut search_from = 0;
    while let Some(pos) = lower[search_from..].find(pseudo) {
      let abs_pos = search_from + pos + pseudo.len();
      // Find matching closing paren
      let mut depth = 1;
      let mut end = abs_pos;
      let chars: Vec<char> = selector.chars().collect();
      while end < chars.len() && depth > 0 {
        match chars[end] {
          '(' => depth += 1,
          ')' => depth -= 1,
          _ => {}
        }
        if depth > 0 {
          end += 1;
        }
      }
      if end > abs_pos {
        let content: String = chars[abs_pos..end].iter().collect();
        // Skip SCSS interpolation — the actual value is dynamic
        if content.contains("#{") || content.contains("@{") {
          search_from = abs_pos;
          continue;
        }
        // The content before any "of" keyword (for :nth-child(An+B of S))
        let anb_part = if let Some(of_pos) = content.to_ascii_lowercase().find(" of ") {
          &content[..of_pos]
        } else {
          &content
        };
        exprs.push(anb_part.trim().to_string());
      }
      search_from = abs_pos;
    }
  }

  exprs
}

impl Rule for SelectorAnbNoUnmatchable {
  fn name(&self) -> &'static str {
    "selector-anb-no-unmatchable"
  }

  fn description(&self) -> &'static str {
    "Disallow unmatchable An+B selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Error
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let mut diags = Vec::new();
    for expr in extract_anb_expressions(&rule.selector) {
      if is_unmatchable(&expr) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected unmatchable An+B selector \"{}\"", expr.trim()),
          )
          .severity(self.default_severity())
          .span(Span::new(rule.span.offset, rule.span.length)),
        );
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{CssNode, Declaration, Span as ParserSpan, StyleRule, Syntax};

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
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_zero() {
    let d = SelectorAnbNoUnmatchable.check(&style_with_selector("li:nth-child(0)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("0"));
  }

  #[test]
  fn reports_0n_plus_0() {
    let d = SelectorAnbNoUnmatchable.check(&style_with_selector("li:nth-child(0n+0)"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_negative_n_plus_0() {
    let d = SelectorAnbNoUnmatchable.check(&style_with_selector("li:nth-child(-n+0)"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_odd() {
    assert!(
      SelectorAnbNoUnmatchable
        .check(&style_with_selector("li:nth-child(odd)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_even() {
    assert!(
      SelectorAnbNoUnmatchable
        .check(&style_with_selector("li:nth-child(even)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_2n_plus_1() {
    assert!(
      SelectorAnbNoUnmatchable
        .check(&style_with_selector("li:nth-child(2n+1)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_n_plus_3() {
    assert!(
      SelectorAnbNoUnmatchable
        .check(&style_with_selector("li:nth-child(n+3)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_negative_n_plus_5() {
    // -n+5 matches elements 1 through 5
    assert!(
      SelectorAnbNoUnmatchable
        .check(&style_with_selector("li:nth-child(-n+5)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_positive_integer() {
    assert!(
      SelectorAnbNoUnmatchable
        .check(&style_with_selector("li:nth-child(3)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn works_with_nth_of_type() {
    let d = SelectorAnbNoUnmatchable.check(&style_with_selector("li:nth-of-type(0)"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn works_with_nth_last_child() {
    let d = SelectorAnbNoUnmatchable.check(&style_with_selector("li:nth-last-child(0)"), &ctx());
    assert_eq!(d.len(), 1);
  }
}
