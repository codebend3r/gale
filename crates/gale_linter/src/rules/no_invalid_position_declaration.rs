use gale_css_parser::{CssNode, Declaration};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::pattern;
use crate::rule::{Rule, RuleContext};

/// Reports declarations in a position where they do not apply: at the root
/// of the stylesheet, or directly inside a conditional at-rule (`@media`,
/// `@supports`, ...) that is not nested in a style rule.
///
/// Equivalent to Stylelint's `no-invalid-position-declaration` rule, which
/// accepts `ignoreAtRules`: at-rules that, as an ancestor of the
/// declaration's at-rule, make its position valid (`["include", "mixin"]`
/// for SCSS content blocks and mixin bodies, say).
pub struct NoInvalidPositionDeclaration;

/// At-rules that allow declarations only when nested inside a style rule
/// (Stylelint's `nestingSupportedAtKeywords`).
const NESTING_SUPPORTED_AT_RULES: &[&str] = &[
  "apply",
  "container",
  "layer",
  "media",
  "scope",
  "starting-style",
  "supports",
];

/// At-rules that allow declarations directly inside them, wherever they are
/// (Stylelint's `declarationContainingAtKeywords`).
const DECLARATION_CONTAINING_AT_RULES: &[&str] = &["mixin", "scope"];

/// Where a run of sibling nodes sits.
#[derive(Clone, Copy)]
enum Parent<'a> {
  /// The stylesheet root.
  Root,
  /// A style rule's block.
  Rule,
  /// An at-rule's block, by at-rule name as written.
  AtRule(&'a str),
}

/// The walk's state: the rule's options and the problems found so far.
struct Walk<'a, 'o> {
  /// The rule, for its name and severity.
  rule: &'a NoInvalidPositionDeclaration,
  /// The `ignoreAtRules` option, a string or a list of strings and
  /// `/regex/` entries.
  ignore_at_rules: Option<&'o serde_json::Value>,
  /// Problems found so far.
  diags: Vec<Diagnostic>,
}

impl Walk<'_, '_> {
  /// Check the declarations among `nodes` and walk into their blocks.
  ///
  /// `shielded` is true when the parent or an ancestor is a style rule, a
  /// declaration-containing at-rule or an ignored at-rule: Stylelint's
  /// `findNodeUpToRoot` test for at-rules that need nesting.
  fn nodes(&mut self, nodes: &[CssNode], parent: Parent<'_>, shielded: bool) {
    for node in nodes {
      match node {
        CssNode::Declaration(decl) => self.declaration(decl, parent, shielded),
        CssNode::AtRule(at) => {
          let shields = shielded
            || is_declaration_containing(&at.name)
            || pattern::option_matches(self.ignore_at_rules, &at.name);
          self.nodes(&at.children, Parent::AtRule(&at.name), shields);
        }
        CssNode::Style(style) => self.style_rule(style),
        CssNode::Comment(_) => {}
      }
    }
  }

  /// Walk a style rule's nested blocks.  Its own declarations are always in
  /// a valid position, and so is anything nested under it.
  fn style_rule(&mut self, style: &gale_css_parser::StyleRule) {
    self.nodes(&style.nested_at_rules, Parent::Rule, true);
    for child in &style.children {
      self.style_rule(child);
    }
  }

  /// Report `decl` when its position is invalid.
  fn declaration(&mut self, decl: &Declaration, parent: Parent<'_>, shielded: bool) {
    if !is_standard_syntax_declaration(&decl.property) {
      return;
    }
    let valid = match parent {
      Parent::Root => false,
      Parent::Rule => true,
      Parent::AtRule(name) => {
        is_declaration_containing(name) || !is_nesting_supported(name) || shielded
      }
    };
    if valid {
      return;
    }
    // The declaration's own span, so a disable comment for the
    // declaration's line covers the report.
    self.diags.push(
      Diagnostic::new(self.rule.name(), "Invalid position for declaration")
        .severity(self.rule.default_severity())
        .span(Span::new(decl.span.offset, decl.span.length)),
    );
  }
}

/// Whether `name` is an at-rule that takes declarations only when nested.
fn is_nesting_supported(name: &str) -> bool {
  NESTING_SUPPORTED_AT_RULES
    .iter()
    .any(|known| known.eq_ignore_ascii_case(name))
}

/// Whether `name` is an at-rule that takes declarations directly.
fn is_declaration_containing(name: &str) -> bool {
  DECLARATION_CONTAINING_AT_RULES
    .iter()
    .any(|known| known.eq_ignore_ascii_case(name))
}

/// Stylelint's `isStandardSyntaxDeclaration` as far as the property tells:
/// SCSS variables (`$var`, `ns.$var`) and Less variables (`@var`, but not
/// `@{var}` interpolation) are not declarations this rule looks at.
fn is_standard_syntax_declaration(property: &str) -> bool {
  let scss_variable = property.starts_with('$') || property.contains(".$");
  let less_variable = property.starts_with('@') && !property.starts_with("@{");
  !scss_variable && !less_variable
}

impl Rule for NoInvalidPositionDeclaration {
  fn name(&self) -> &'static str {
    "no-invalid-position-declaration"
  }

  fn description(&self) -> &'static str {
    "Disallow invalid position declarations"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags declarations at the root, or directly inside an at-rule that
  /// takes them only when nested in a style rule.
  fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut walk = Walk {
      rule: self,
      ignore_at_rules: ctx
        .secondary_options()
        .and_then(|secondary| secondary.get("ignoreAtRules")),
      diags: Vec::new(),
    };
    walk.nodes(nodes, Parent::Root, false);
    walk.diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Declaration, Span as ParserSpan, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_declaration_inside_media() {
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![CssNode::Declaration(Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      })],
    });
    let d = NoInvalidPositionDeclaration.check_root(&[node], &ctx());
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "Invalid position for declaration");
  }

  #[test]
  fn allows_style_rules_inside_media() {
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![CssNode::Style(gale_css_parser::StyleRule {
        selector: "a".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(0, 0),
          important: false,
        }],
        span: ParserSpan::new(0, 0),
        ..Default::default()
      })],
    });
    let d = NoInvalidPositionDeclaration.check_root(&[node], &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_scss_variable_inside_media() {
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![CssNode::Declaration(Declaration {
        property: "$sidebar-width".to_string(),
        value: "285px".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      })],
    });
    let d = NoInvalidPositionDeclaration.check_root(&[node], &ctx());
    assert!(d.is_empty(), "SCSS variables should not be flagged");
  }

  #[test]
  fn allows_declarations_inside_font_face() {
    let node = CssNode::AtRule(AtRule {
      name: "font-face".to_string(),
      params: String::new(),
      span: ParserSpan::new(0, 0),
      children: vec![CssNode::Declaration(Declaration {
        property: "font-family".to_string(),
        value: "MyFont".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      })],
    });
    let d = NoInvalidPositionDeclaration.check_root(&[node], &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_scss_nested_media_with_declarations() {
    // In SCSS, @media nested inside a style rule legitimately contains
    // declarations scoped by the parent selector.  This should NOT be flagged.
    let node = CssNode::Style(gale_css_parser::StyleRule {
      selector: ".foo".to_string(),
      declarations: vec![],
      span: ParserSpan::new(0, 100),
      nested_at_rules: vec![CssNode::AtRule(AtRule {
        name: "media".to_string(),
        params: "(min-width: 600px)".to_string(),
        span: ParserSpan::new(10, 80),
        children: vec![CssNode::Declaration(Declaration {
          property: "font-size".to_string(),
          value: "14px".to_string(),
          span: ParserSpan::new(40, 16),
          important: false,
        })],
      })],
      ..Default::default()
    });
    let d = NoInvalidPositionDeclaration.check_root(&[node], &ctx());
    assert!(
      d.is_empty(),
      "declarations inside @media nested in a style rule should not be flagged"
    );
  }

  /// Lint `source` with the real parser and return each report's line.
  fn lines(source: &str, syntax: Syntax, options: Option<serde_json::Value>) -> Vec<usize> {
    let parsed = gale_css_parser::parse(source, syntax).expect("parses");
    let ctx = RuleContext {
      file_path: "t.scss",
      source,
      syntax,
      options: options.as_ref(),
    };
    NoInvalidPositionDeclaration
      .check_root(&parsed.nodes, &ctx)
      .iter()
      .map(|d| source[..d.span.offset].lines().count().max(1))
      .collect()
  }

  #[test]
  fn mixin_bodies_take_declarations_in_nested_media() {
    let source = "@mixin reduce-motion {\n  @media (prefers-reduced-motion: reduce) {\n    animation: none;\n  }\n}\n";
    assert!(lines(source, Syntax::Scss, None).is_empty());
  }

  #[test]
  fn root_content_blocks_need_ignore_at_rules() {
    let source =
      "@include pf-root($x) {\n  --a: 0;\n  @media (min-width: 1px) {\n    --b: 1;\n  }\n}\n";
    // `@include` is no style rule, so the `@media` inside it is at the root.
    assert_eq!(lines(source, Syntax::Scss, None), vec![4]);
    // As in patternfly's config, ignoring the at-rule makes it valid.
    let ignore = serde_json::json!([true, { "ignoreAtRules": ["include", "mixin"] }]);
    assert!(lines(source, Syntax::Scss, Some(ignore)).is_empty());
    let regex = serde_json::json!([true, { "ignoreAtRules": ["/^incl/"] }]);
    assert!(lines(source, Syntax::Scss, Some(regex)).is_empty());
  }

  #[test]
  fn ignoring_the_parent_at_rule_itself_is_enough() {
    let source = "@media (min-width: 1px) {\n  color: red;\n}\n";
    assert_eq!(lines(source, Syntax::Scss, None), vec![2]);
    let ignore = serde_json::json!([true, { "ignoreAtRules": "media" }]);
    assert!(lines(source, Syntax::Scss, Some(ignore)).is_empty());
  }

  #[test]
  fn declarations_in_other_at_rules_and_nested_blocks_are_valid() {
    let source = "@include foo {\n  color: red;\n}\n@if $x {\n  color: red;\n}\n.a {\n  @include foo {\n    @media (x) {\n      color: teal;\n    }\n  }\n}\n@media (x) {\n  $local: 1;\n}\n";
    assert!(lines(source, Syntax::Scss, None).is_empty());
  }

  #[test]
  fn root_level_declarations_are_reported() {
    let source = "color: red;\n$x: 1;\n.a {\n  color: blue;\n}\n";
    assert_eq!(lines(source, Syntax::Scss, None), vec![1]);
  }
}
