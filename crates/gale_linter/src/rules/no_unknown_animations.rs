use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Warn when an `animation-name` or `animation` shorthand references a name
/// that doesn't have a corresponding `@keyframes` definition in the same file.
///
/// Equivalent to Stylelint's `no-unknown-animations` rule.
/// Detection-only (no autofix).
pub struct NoUnknownAnimations;

impl Rule for NoUnknownAnimations {
  fn name(&self) -> &'static str {
    "no-unknown-animations"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown animation names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    // First pass: collect all @keyframes names.
    let mut keyframe_names = Vec::new();
    collect_keyframe_names(nodes, &mut keyframe_names);

    let is_preprocessor = matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass | gale_css_parser::Syntax::Less
    );

    // Second pass: find animation-name and animation declarations.
    let mut diags = Vec::new();
    collect_animation_issues(nodes, &keyframe_names, self, &mut diags, is_preprocessor);
    diags
  }
}

fn collect_keyframe_names(nodes: &[CssNode], names: &mut Vec<String>) {
  for node in nodes {
    if let CssNode::AtRule(at_rule) = node {
      if at_rule.name == "keyframes" && !at_rule.params.is_empty() {
        names.push(at_rule.params.clone());
      }
      collect_keyframe_names(&at_rule.children, names);
    }
  }
}

fn collect_animation_issues(
  nodes: &[CssNode],
  keyframe_names: &[String],
  rule: &NoUnknownAnimations,
  diags: &mut Vec<Diagnostic>,
  is_preprocessor: bool,
) {
  for node in nodes {
    match node {
      CssNode::Style(style_rule) => {
        for decl in &style_rule.declarations {
          let prop = decl.property.to_ascii_lowercase();
          if prop == "animation-name" {
            // Value may be comma-separated list of names.
            for name in decl.value.split(',') {
              let name = name.trim();
              // Skip names with SCSS/Less interpolation — we can't
              // resolve the actual name at lint time.
              if is_preprocessor && name.contains("#{") {
                continue;
              }
              if !name.is_empty() && name != "none" && !keyframe_names.contains(&name.to_string()) {
                diags.push(
                  Diagnostic::new(rule.name(), format!("Unknown animation name \"{name}\""))
                    .severity(rule.default_severity())
                    .span(Span::new(decl.span.offset, decl.span.length)),
                );
              }
            }
          } else if prop == "animation" {
            // Skip values with SCSS/Less interpolation.
            if is_preprocessor && decl.value.contains("#{") {
              continue;
            }
            // The animation shorthand: try to extract the animation name.
            // The name is typically the first value that isn't a time,
            // easing function, iteration count, etc.
            if let Some(name) = extract_animation_name(&decl.value)
              && name != "none"
              && !keyframe_names.contains(&name.to_string())
            {
              diags.push(
                Diagnostic::new(rule.name(), format!("Unknown animation name \"{name}\""))
                  .severity(rule.default_severity())
                  .span(Span::new(decl.span.offset, decl.span.length)),
              );
            }
          }
        }
        // Recurse into nested rules.
        for child in &style_rule.children {
          collect_animation_issues(
            &[CssNode::Style(child.clone())],
            keyframe_names,
            rule,
            diags,
            is_preprocessor,
          );
        }
      }
      CssNode::AtRule(at_rule) => {
        collect_animation_issues(
          &at_rule.children,
          keyframe_names,
          rule,
          diags,
          is_preprocessor,
        );
      }
      _ => {}
    }
  }
}

/// Known animation shorthand keywords/values that are NOT animation names.
const ANIMATION_KEYWORDS: &[&str] = &[
  "ease",
  "ease-in",
  "ease-out",
  "ease-in-out",
  "linear",
  "step-start",
  "step-end",
  "normal",
  "reverse",
  "alternate",
  "alternate-reverse",
  "forwards",
  "backwards",
  "both",
  "running",
  "paused",
  "infinite",
  "none",
];

/// Try to extract the animation name from an `animation` shorthand value.
/// Returns the first token that isn't a known keyword, time value, or number.
fn extract_animation_name(value: &str) -> Option<String> {
  for token in value.split_whitespace() {
    let lower = token.to_ascii_lowercase();
    // Skip time values (e.g. 1s, 200ms, 0.5s)
    if lower.ends_with('s') || lower.ends_with("ms") {
      let num_part = lower.trim_end_matches("ms").trim_end_matches('s');
      if num_part.parse::<f64>().is_ok() {
        continue;
      }
    }
    // Skip pure numbers (iteration count)
    if token.parse::<f64>().is_ok() {
      continue;
    }
    // Skip known keywords
    if ANIMATION_KEYWORDS.contains(&lower.as_str()) {
      continue;
    }
    // Skip cubic-bezier(...) and steps(...)
    if lower.starts_with("cubic-bezier(") || lower.starts_with("steps(") {
      continue;
    }
    return Some(token.to_string());
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_unknown_animation_name() {
    let nodes = vec![CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "animation-name".to_string(),
        value: "fadeIn".to_string(),
        span: ParserSpan::new(4, 20),
        important: false,
      }],
      span: ParserSpan::new(0, 30),
      ..Default::default()
    })];
    let d = NoUnknownAnimations.check_root(&nodes, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("fadeIn"));
  }

  #[test]
  fn allows_known_animation_name() {
    let nodes = vec![
      CssNode::AtRule(AtRule {
        name: "keyframes".to_string(),
        params: "fadeIn".to_string(),
        span: ParserSpan::new(0, 40),
        children: vec![],
      }),
      CssNode::Style(StyleRule {
        selector: "a".to_string(),
        declarations: vec![Declaration {
          property: "animation-name".to_string(),
          value: "fadeIn".to_string(),
          span: ParserSpan::new(50, 20),
          important: false,
        }],
        span: ParserSpan::new(45, 30),
        ..Default::default()
      }),
    ];
    let d = NoUnknownAnimations.check_root(&nodes, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_unknown_in_animation_shorthand() {
    let nodes = vec![CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "animation".to_string(),
        value: "slideUp 1s ease".to_string(),
        span: ParserSpan::new(4, 25),
        important: false,
      }],
      span: ParserSpan::new(0, 35),
      ..Default::default()
    })];
    let d = NoUnknownAnimations.check_root(&nodes, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("slideUp"));
  }
}
