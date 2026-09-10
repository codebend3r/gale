use std::collections::HashSet;

use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow duplicate `@mixin` declarations with the same name.
pub struct ScssNoDuplicateMixins;

impl Rule for ScssNoDuplicateMixins {
  fn name(&self) -> &'static str {
    "scss/no-duplicate-mixins"
  }

  fn description(&self) -> &'static str {
    "Disallow duplicate mixin names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut diagnostics = Vec::new();
    collect_mixins(nodes, &mut seen, &mut diagnostics, self, ctx.source);
    diagnostics
  }
}

fn collect_mixins(
  nodes: &[CssNode],
  seen: &mut HashSet<String>,
  diagnostics: &mut Vec<Diagnostic>,
  rule: &ScssNoDuplicateMixins,
  source: &str,
) {
  for node in nodes {
    match node {
      CssNode::AtRule(at) if at.name == "mixin" => {
        // The mixin name is everything before the first `(` or whitespace.
        let raw = at.params.trim();
        let mixin_name = raw
          .split(|c: char| c == '(' || c.is_whitespace())
          .next()
          .unwrap_or("")
          .trim();
        if mixin_name.is_empty() {
          continue;
        }
        if seen.contains(mixin_name) {
          // Point to the mixin name, not the `@mixin` keyword.
          // Stylelint message has no quotes around the name.
          let at_src_end = (at.span.offset + at.span.length).min(source.len());
          let at_src = &source[at.span.offset..at_src_end];
          let name_off = at_src.find(mixin_name).unwrap_or(0);
          diagnostics.push(
            Diagnostic::new(
              rule.name(),
              format!("Unexpected duplicate mixin {}", mixin_name),
            )
            .severity(rule.default_severity())
            .span(Span::new(at.span.offset + name_off, mixin_name.len())),
          );
        } else {
          seen.insert(mixin_name.to_string());
        }
        // Also recurse into children for nested mixins.
        collect_mixins(&at.children, seen, diagnostics, rule, source);
      }
      CssNode::AtRule(at) => {
        collect_mixins(&at.children, seen, diagnostics, rule, source);
      }
      _ => {}
    }
  }
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

  fn css_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn mixin(name: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "mixin".to_string(),
      params: name.to_string(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    })
  }

  #[test]
  fn skips_non_scss() {
    let nodes = vec![mixin("foo"), mixin("foo")];
    assert!(
      ScssNoDuplicateMixins
        .check_root(&nodes, &css_ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_duplicate_mixins() {
    let nodes = vec![mixin("foo"), mixin("bar"), mixin("foo")];
    let d = ScssNoDuplicateMixins.check_root(&nodes, &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("foo"));
  }

  #[test]
  fn allows_unique_mixins() {
    let nodes = vec![mixin("foo"), mixin("bar"), mixin("baz")];
    let d = ScssNoDuplicateMixins.check_root(&nodes, &scss_ctx());
    assert!(d.is_empty());
  }
}
