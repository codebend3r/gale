use std::collections::HashSet;

use gale_css_parser::{CssNode, Declaration, StyleRule};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};
use crate::source_text;
use crate::stylelint_version::stylelint_major_version;
use crate::value_parser::{self, NodeKind, ValueNode};

/// Reports a custom property (e.g. `--my-color`) used as a value without a
/// `var()` around it.
///
/// Equivalent to Stylelint's `custom-property-no-missing-var-function`
/// rule.  Like it, only names this stylesheet defines (with a `--name:`
/// declaration or an `@property --name` rule) count as custom properties:
/// any other dashed ident, an anchor name in `anchor-name: --menu` say, is
/// left alone, as are properties that take a custom ident.
pub struct CustomPropertyNoMissingVarFunction;

/// Properties whose values may hold a dashed ident that is no custom
/// property reference (Stylelint's `IGNORED_PROPERTIES`).
const IGNORED_PROPERTIES: &[&str] = &[
  "animation",
  "animation-name",
  "container-name",
  "counter-increment",
  "counter-reset",
  "counter-set",
  "grid-column",
  "grid-column-end",
  "grid-column-start",
  "grid-row",
  "grid-row-end",
  "grid-row-start",
  "list-style",
  "list-style-type",
  "transition",
  "transition-property",
  "view-transition-name",
  "will-change",
];

/// Properties Stylelint 17 added to [`IGNORED_PROPERTIES`].
const IGNORED_PROPERTIES_SINCE_17: &[&str] = &["animation-timeline", "timeline-scope"];

/// Every declaration in `nodes`, at any depth, and the params of every
/// `@property` rule.
fn collect<'a>(
  nodes: &'a [CssNode],
  decls: &mut Vec<&'a Declaration>,
  property_rules: &mut Vec<&'a gale_css_parser::AtRule>,
) {
  for node in nodes {
    match node {
      CssNode::Declaration(decl) => decls.push(decl),
      CssNode::AtRule(at) => {
        if at.name.eq_ignore_ascii_case("property") {
          property_rules.push(at);
        }
        collect(&at.children, decls, property_rules);
      }
      CssNode::Style(rule) => collect_rule(rule, decls, property_rules),
      CssNode::Comment(_) => {}
    }
  }
}

/// [`collect`] for a style rule and everything nested in it.
fn collect_rule<'a>(
  rule: &'a StyleRule,
  decls: &mut Vec<&'a Declaration>,
  property_rules: &mut Vec<&'a gale_css_parser::AtRule>,
) {
  decls.extend(rule.declarations.iter());
  collect(&rule.nested_at_rules, decls, property_rules);
  for child in &rule.children {
    collect_rule(child, decls, property_rules);
  }
}

/// The checks for one stylesheet.
struct Check<'a> {
  /// Custom property names the stylesheet defines.
  known: HashSet<&'a str>,
  /// Where each reported dashed ident starts in the source, and its name.
  found: Vec<(usize, &'a str)>,
}

impl<'a> Check<'a> {
  /// Check a value node whose value text starts at `offset` in the source.
  fn node(&mut self, node: &ValueNode<'a>, offset: usize) {
    if node.is_function() {
      let name = node.value.to_ascii_lowercase();
      let args: &[ValueNode<'a>] = match name.as_str() {
        "var" => node.nodes.get(1..).unwrap_or_default(),
        "running" => match node.nodes.first() {
          Some(first) if first.is_function() && first.value.eq_ignore_ascii_case("var") => {
            first.nodes.get(1..).unwrap_or_default()
          }
          _ => return,
        },
        "style" => {
          // In a `style()` query the property name before the `:` is
          // exempt; the value after it is checked as usual.
          let mut after_colon = false;
          for arg in &node.nodes {
            if arg.kind == NodeKind::Div && arg.value == ":" {
              after_colon = true;
            } else if after_colon {
              self.node(arg, offset);
            }
          }
          return;
        }
        _ => &node.nodes,
      };
      for arg in args {
        self.node(arg, offset);
      }
      return;
    }
    if !node.is_word() || !node.value.starts_with("--") {
      return;
    }
    // `postcss-value-parser` keeps a trailing `;` in a word.
    let name = node.value.trim_end_matches(';');
    if self.known.contains(name) {
      self.found.push((offset + node.source_index, name));
    }
  }
}

impl Rule for CustomPropertyNoMissingVarFunction {
  fn name(&self) -> &'static str {
    "custom-property-no-missing-var-function"
  }

  fn description(&self) -> &'static str {
    "Disallow missing var function for custom properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags a custom property this stylesheet defines when a value uses it
  /// bare instead of inside `var()`.
  fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut decls = Vec::new();
    let mut property_rules = Vec::new();
    collect(nodes, &mut decls, &mut property_rules);

    let mut check = Check {
      known: HashSet::new(),
      found: Vec::new(),
    };
    for at in property_rules {
      let params =
        source_text::at_rule_params(ctx.source, at).map_or(at.params.trim(), |(params, _)| params);
      check.known.insert(params);
    }
    for decl in &decls {
      if decl.property.starts_with("--") {
        check.known.insert(decl.property.as_str());
      }
    }
    if check.known.is_empty() {
      return vec![];
    }

    let stylelint_17 = stylelint_major_version() >= 17;
    for decl in decls {
      let Some((value, offset)) = source_text::declaration_value(ctx.source, decl) else {
        continue;
      };
      if !value.contains("--") {
        continue;
      }
      let property = decl.property.to_ascii_lowercase();
      let ignored = IGNORED_PROPERTIES.contains(&property.as_str())
        || (stylelint_17 && IGNORED_PROPERTIES_SINCE_17.contains(&property.as_str()));
      if ignored {
        continue;
      }
      for node in &value_parser::parse(value) {
        check.node(node, offset);
      }
    }

    check
      .found
      .into_iter()
      .map(|(offset, name)| {
        let message = if stylelint_17 {
          format!("Missing var function for \"{name}\"")
        } else {
          format!("Unexpected missing var function for \"{name}\"")
        };
        Diagnostic::new(self.name(), message)
          .severity(self.default_severity())
          .span(Span::new(offset, name.len()))
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::ctx_with_source;

  /// Lint `source` with the real parser, returning `(offset, name)` for
  /// each report (the name is the quoted part of the message).
  fn lint(source: &str) -> Vec<(usize, String)> {
    let parsed = gale_css_parser::parse(source, Syntax::Css).expect("parses");
    let ctx = ctx_with_source(source);
    CustomPropertyNoMissingVarFunction
      .check_root(&parsed.nodes, &ctx)
      .into_iter()
      .map(|d| {
        let name = d.message.split('"').nth(1).unwrap_or_default().to_string();
        (d.span.offset, name)
      })
      .collect()
  }

  #[test]
  fn reports_a_defined_custom_property_used_bare() {
    let source = ":root { --accent: red; }\na { color: --accent; }";
    assert_eq!(lint(source), vec![(36, "--accent".to_string())]);
  }

  #[test]
  fn names_the_property_in_the_message() {
    let source = ":root { --a: 1px; }\na { margin: 0 --a; }";
    let parsed = gale_css_parser::parse(source, Syntax::Css).expect("parses");
    let ctx = ctx_with_source(source);
    let d = CustomPropertyNoMissingVarFunction.check_root(&parsed.nodes, &ctx);
    assert_eq!(d.len(), 1);
    assert!(
      d[0].message.ends_with("var function for \"--a\""),
      "{}",
      d[0].message
    );
  }

  #[test]
  fn leaves_undefined_dashed_idents_alone() {
    // Anchor names are dashed idents, not custom property references.
    let source = ".a { anchor-name: --menu; }\n.b { position-anchor: --menu; }";
    assert!(lint(source).is_empty());
  }

  #[test]
  fn var_fallbacks_and_property_rules_count() {
    let source = "@property --size { syntax: '<length>'; inherits: false; initial-value: 0px; }\na { width: var(--other, --size); height: var(--size); }";
    assert_eq!(lint(source), vec![(102, "--size".to_string())]);
  }

  #[test]
  fn properties_taking_custom_idents_are_skipped() {
    let source = ":root { --fade: 1; }\na { animation-name: --fade; transition-property: --fade; }";
    assert!(lint(source).is_empty());
  }
}
