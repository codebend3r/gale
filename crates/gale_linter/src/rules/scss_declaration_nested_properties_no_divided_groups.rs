use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::postcss_tree::NodeKind;
use crate::rule::{LegacyRule as Rule, RuleContext};

/// Disallow nested properties of one namespace split into several groups
/// in a block (`background: red { ... }` and later `background: { ... }`).
///
/// Equivalent to stylelint-scss's
/// `scss/declaration-nested-properties-no-divided-groups`.  Each child of a
/// rule or at-rule that opens a nested group is read with upstream's
/// `parseNestedPropRoot`: a nested declaration (`margin: 0 { ... }`) as
/// `prop: value`, a rule (`background: { ... }`, or any rule in plain CSS)
/// by its selector.  When two or more in one block share a namespace, the
/// namespace is reported in each of them.
pub struct ScssDeclarationNestedPropertiesNoDividedGroups;

impl Rule for ScssDeclarationNestedPropertiesNoDividedGroups {
  fn name(&self) -> &'static str {
    "scss/declaration-nested-properties-no-divided-groups"
  }

  fn description(&self) -> &'static str {
    "Disallow divided groups in nested property declarations"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports every group of a namespace that has more than one group in
  /// the same block.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let tree = ctx.postcss_tree();
    let mut diags = Vec::new();

    for parent in &tree.nodes {
      if !matches!(parent.kind, NodeKind::Rule | NodeKind::AtRule) {
        continue;
      }
      let Some(children) = &parent.children else {
        continue;
      };
      // Namespaces in the order they first appear, with their groups.
      let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
      for &child in children {
        let node = &tree.nodes[child];
        let root = match node.kind {
          NodeKind::Rule => node.name.clone(),
          NodeKind::Decl if node.children.is_some() => format!("{}: {}", node.name, node.value),
          _ => continue,
        };
        let Some(namespace) = nested_prop_root(&root) else {
          continue;
        };
        match groups.iter_mut().find(|(name, _)| name == namespace) {
          Some((_, nodes)) => nodes.push(child),
          None => groups.push((namespace.to_string(), vec![child])),
        }
      }

      for (namespace, nodes) in groups.iter().filter(|(_, nodes)| nodes.len() > 1) {
        for &child in nodes {
          // Upstream points at the first occurrence of the namespace in the
          // node's text.
          let node = &tree.nodes[child];
          let span = tree.text(child).find(namespace.as_str()).map_or_else(
            || Span::from_range(node.start, node.end),
            |at| Span::new(node.start + at, namespace.len()),
          );
          diags.push(
            Diagnostic::new(
              self.name(),
              format!(
                "Expected all nested properties of \"{namespace}\" namespace to be in one nested group"
              ),
            )
            .severity(self.default_severity())
            .message_args([namespace.as_str()])
            .span(span),
          );
        }
      }
    }
    diags
  }
}

/// stylelint-scss's `parseNestedPropRoot`: the property name of a nested
/// property group's root (`margin: {`, `font: 10px/1.1 Arial {`, the `{`
/// left out), or `None` when `text` reads as a selector.
///
/// The name ends at the first `:` outside strings and braces.  A value
/// after it makes a root only when whitespace follows the colon or the
/// value starts like a number, a variable or a string; `a:hover` and
/// `&:not(.b)` are selectors.
fn nested_prop_root(text: &str) -> Option<&str> {
  /// What the scan is inside.
  #[derive(PartialEq)]
  enum Mode {
    Normal,
    Braces,
    String(char),
  }
  let mut stack = vec![Mode::Normal];
  let mut prev = None;
  for (i, c) in text.char_indices() {
    let escaped = prev == Some('\\');
    prev = Some(c);
    match c {
      '"' | '\'' => {
        match stack.last() {
          Some(Mode::String(quote)) if *quote == c && !escaped => {
            stack.pop();
          }
          Some(Mode::String(_)) => {}
          _ => stack.push(Mode::String(c)),
        }
        continue;
      }
      '{' => {
        stack.push(Mode::Braces);
        continue;
      }
      '}' => {
        stack.pop();
        continue;
      }
      _ => {}
    }
    if stack.last() != Some(&Mode::Normal) || c != ':' || escaped {
      continue;
    }
    let name = text[..i].trim();
    let raw_value = &text[i + 1..];
    let value = raw_value.trim();
    let spaced = raw_value.len() != raw_value.trim_start().len();
    let looks_like_value =
      value.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '.' | '$' | '\'' | '"'));
    if !raw_value.is_empty() && !spaced && !looks_like_value {
      return None;
    }
    return Some(name);
  }
  None
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use gale_diagnostics::SourceLineIndex;
  use serde_json::json;

  use super::nested_prop_root;
  use crate::testing::lint;

  const RULE: &str = "scss/declaration-nested-properties-no-divided-groups";

  /// The `(line, column, endLine, endColumn)` of each warning for `source`,
  /// all of which must be about `namespace`.
  fn ranges(source: &str, syntax: Syntax, namespace: &str) -> Vec<(usize, usize, usize, usize)> {
    let index = SourceLineIndex::build(source);
    lint(RULE, json!(true), source, syntax)
      .iter()
      .map(|d| {
        assert_eq!(
          d.message,
          format!(
            "Expected all nested properties of \"{namespace}\" namespace to be in one nested group"
          )
        );
        let (line, column) = index.offset_to_location(d.span.offset);
        let (end_line, end_column) = index.offset_to_location(d.span.end());
        (line, column, end_line, end_column)
      })
      .collect()
  }

  /// `(line, column)` of each `background` warning for `source`.
  fn starts(source: &str, syntax: Syntax) -> Vec<(usize, usize)> {
    ranges(source, syntax, "background")
      .into_iter()
      .map(|(line, column, _, _)| (line, column))
      .collect()
  }

  /// Upstream runs every case under both the default syntax and
  /// postcss-scss.
  const SYNTAXES: [Syntax; 2] = [Syntax::Css, Syntax::Scss];

  #[test]
  fn accepts_upstream_cases() {
    for source in [
      "\n      a {\n        background: no-repeat;\n        margin: 1px;\n      }\n    ",
      "\n      a {\n        background: url(img.png) center;\n        background-repeat: no-repeat;\n      }\n    ",
      "\n      a {\n        background: url(img.png) center {\n          size: auto;\n        }\n        background-repeat: no-repeat;\n      }\n    ",
      "\n      a {\n        background: url(img.png) center {\n          size: auto;\n        }\n        background-color: red;\n        background-repeat: no-repeat;\n      }\n    ",
      "\n      a {\n        background: url(img.png) center {\n          size: auto;\n        }\n        background :red {\n          repeat: no-repeat;\n        }\n      }\n    ",
      "\n      a {\n        background: url(img.png) center {\n          size: auto;\n        }\n        margin: 10px {\n          left: 1px;\n        }\n      }\n    ",
      "\n      a {\n        background: url(img.png) center {\n          size: auto;\n        }\n        @media (color) {\n          background: red {\n            repeat: no-repeat;\n          }\n        }\n      }\n    ",
      "\n      a {\n        &:not(.other-class) { }\n        &:not(.another-class) { }\n      }\n    ",
    ] {
      for syntax in SYNTAXES {
        assert_eq!(starts(source, syntax), vec![], "{syntax:?}: {source}");
      }
    }
  }

  #[test]
  fn reports_two_groups_of_one_namespace() {
    let source = "\n      a {\n        background: url(img.png) center {\n          size: auto;\n        }\n        background : red {\n          repeat: no-repeat;\n        }\n      }\n    ";
    for syntax in SYNTAXES {
      assert_eq!(
        ranges(source, syntax, "background"),
        vec![(3, 9, 3, 19), (6, 9, 6, 19)],
        "{syntax:?}"
      );
    }
  }

  #[test]
  fn reports_a_second_group_without_a_value() {
    let source = "\n      a {\n        background: url(img.png) center {\n          size: auto;\n        }\n        background : {\n          repeat: no-repeat;\n        }\n      }\n    ";
    for syntax in SYNTAXES {
      assert_eq!(starts(source, syntax), vec![(3, 9), (6, 9)], "{syntax:?}");
    }
  }

  #[test]
  fn reports_groups_divided_by_another_namespace() {
    let source = "\n      a {\n        background: url(img.png) center {\n          size: auto;\n        }\n        margin: 10px {\n          left: 1px;\n        }\n\n        background: linear-gradient(to bottom, red 0%, blue 10%) {\n          position: center;\n        }\n      }\n    ";
    for syntax in SYNTAXES {
      assert_eq!(starts(source, syntax), vec![(3, 9), (10, 9)], "{syntax:?}");
    }
  }

  #[test]
  fn reports_three_groups_of_one_namespace() {
    let source = "\n      a {\n        background: url(img.png) center {\n          size: auto;\n        }\n        background: red {\n          repeat: no-repeat;\n        }\n\n        background: linear-gradient(to bottom, red 0%, blue 10%) {\n          position: center;\n        }\n      }\n    ";
    for syntax in SYNTAXES {
      assert_eq!(
        starts(source, syntax),
        vec![(3, 9), (6, 9), (10, 9)],
        "{syntax:?}"
      );
    }
  }

  #[test]
  fn reads_nested_property_roots_like_upstream() {
    assert_eq!(nested_prop_root("margin:"), Some("margin"));
    assert_eq!(nested_prop_root("font: 10px/1.1 Arial"), Some("font"));
    assert_eq!(nested_prop_root("margin:10px"), Some("margin"));
    assert_eq!(nested_prop_root("margin:$a"), Some("margin"));
    assert_eq!(nested_prop_root("a:hover"), None);
    assert_eq!(nested_prop_root("&:not(.b)"), None);
    assert_eq!(nested_prop_root("#{$a}:hover"), None);
    assert_eq!(nested_prop_root("a[href='x:y']"), None);
    assert_eq!(nested_prop_root(".a"), None);
  }
}
