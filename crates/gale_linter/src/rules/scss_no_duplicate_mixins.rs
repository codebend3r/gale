use std::collections::HashSet;

use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/no-duplicate-mixins";

declare_rule! {
  /// Disallow duplicate `@mixin` declarations with the same name.
  pub struct ScssNoDuplicateMixins => ScssNoDuplicateMixinsCheck {
    name: NAME,
    description: "Disallow duplicate mixin names",
    default_severity: Warning,
  }
}

/// `scss/no-duplicate-mixins`, which takes no options.
pub struct ScssNoDuplicateMixinsCheck;

impl ScssNoDuplicateMixinsCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssNoDuplicateMixinsCheck {
  /// Flags a `@mixin` name declared more than once.
  fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut diagnostics = Vec::new();
    collect_mixins(nodes, &mut seen, &mut diagnostics, ctx.source);
    diagnostics
  }
}

/// Recurses through the tree, reporting each repeat of a mixin name. The name
/// is everything before the first `(` or space.
fn collect_mixins(
  nodes: &[CssNode],
  seen: &mut HashSet<String>,
  diagnostics: &mut Vec<Diagnostic>,
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
            Diagnostic::new(NAME, format!("Unexpected duplicate mixin {}", mixin_name))
              .span(Span::new(at.span.offset + name_off, mixin_name.len())),
          );
        } else {
          seen.insert(mixin_name.to_string());
        }
        // Also recurse into children for nested mixins.
        collect_mixins(&at.children, seen, diagnostics, source);
      }
      CssNode::AtRule(at) => {
        collect_mixins(&at.children, seen, diagnostics, source);
      }
      _ => {}
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::AtRule;

  use crate::testing::{configured, ctx, invalid_options, scss_ctx};

  fn mixin(name: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "mixin".to_string(),
      params: name.to_string(),
      span: Span::new(0, 10),
      children: vec![],
    })
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssNoDuplicateMixins, None);
    let nodes = vec![mixin("foo"), mixin("foo")];
    assert!(rule.check_root(&nodes, &ctx()).is_empty());
  }

  #[test]
  fn reports_duplicate_mixins() {
    let rule = configured(&ScssNoDuplicateMixins, None);
    let nodes = vec![mixin("foo"), mixin("bar"), mixin("foo")];
    let d = rule.check_root(&nodes, &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("foo"));
  }

  #[test]
  fn allows_unique_mixins() {
    let rule = configured(&ScssNoDuplicateMixins, None);
    let nodes = vec![mixin("foo"), mixin("bar"), mixin("baz")];
    let d = rule.check_root(&nodes, &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/no-duplicate-mixins\""]
    );
  }
}
