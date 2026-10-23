use gale_css_parser::postcss_tree::{DeclRef, NodeRef, PostcssTree};
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "no-invalid-position-declaration";

declare_rule! {
  /// Reports declarations in a position where they do not apply: at the root
  /// of the stylesheet, or directly inside a conditional at-rule (`@media`,
  /// `@supports`, ...) that is not nested in a style rule.
  ///
  /// Equivalent to Stylelint's `no-invalid-position-declaration` rule, which
  /// accepts `ignoreAtRules`: at-rules that, as an ancestor of the
  /// declaration's at-rule, make its position valid (`["include", "mixin"]`
  /// for SCSS content blocks and mixin bodies, say).
  pub struct NoInvalidPositionDeclaration => NoInvalidPositionDeclarationCheck {
    name: NAME,
    description: "Disallow invalid position declarations",
    default_severity: Warning,
  }
}

/// `no-invalid-position-declaration` as configured.
pub struct NoInvalidPositionDeclarationCheck {
  /// `ignoreAtRules`: names or `/regex/` entries.
  ignore_at_rules: Matchers,
}

impl NoInvalidPositionDeclarationCheck {
  /// Stylelint's options: `true`, then `ignoreAtRules`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.primary_true();
    let ignore_at_rules = options.matchers("ignoreAtRules");
    options.finish()?;
    Ok(Self { ignore_at_rules })
  }
}

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

/// Whether `name` is an at-rule that takes declarations only when nested.
fn is_nesting_supported(name: &str) -> bool {
  NESTING_SUPPORTED_AT_RULES
    .iter()
    .any(|known| known.eq_ignore_ascii_case(name))
}

/// Whether `node` is an at-rule that takes declarations directly.
fn is_declaration_containing(node: NodeRef) -> bool {
  node.as_at_rule().is_some_and(|at| {
    DECLARATION_CONTAINING_AT_RULES
      .iter()
      .any(|known| known.eq_ignore_ascii_case(at.name()))
  })
}

impl NoInvalidPositionDeclarationCheck {
  /// Whether `decl` sits where a declaration applies: in a style rule or an
  /// at-rule that takes declarations, or in one that takes them nested
  /// (`@media`, `@supports`, ...) with a style rule, a declaration-taking
  /// at-rule or an ignored at-rule around it.
  fn is_valid_position(&self, decl: DeclRef) -> bool {
    let Some(parent) = decl.parent() else {
      return false;
    };
    if parent.as_rule().is_some() || is_declaration_containing(parent) {
      return true;
    }
    let Some(at) = parent.as_at_rule() else {
      // A nested property's block, say.
      return false;
    };
    if !is_nesting_supported(at.name()) {
      return true;
    }
    decl.ancestors().any(|node| {
      node.as_rule().is_some()
        || is_declaration_containing(node)
        || node
          .as_at_rule()
          .is_some_and(|at| self.ignore_at_rules.matches(at.name()))
    })
  }
}

impl Check for NoInvalidPositionDeclarationCheck {
  /// Flags declarations at the root, or directly inside an at-rule that
  /// takes them only when nested in a style rule.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    tree
      .walk_decls()
      .filter(|decl| decl.is_standard_syntax() && !self.is_valid_position(*decl))
      .map(|decl| Diagnostic::new(NAME, "Invalid position for declaration").span(decl.span()))
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context};

  /// Lint `source` and return each report's line.
  fn lines(source: &str, syntax: Syntax, options: Option<serde_json::Value>) -> Vec<usize> {
    configured(&NoInvalidPositionDeclaration, options)
      .check_source(&context(source, syntax))
      .iter()
      .map(|d| source[..d.span.offset].lines().count().max(1))
      .collect()
  }

  #[test]
  fn reports_declaration_inside_media() {
    let source = "@media (min-width: 768px) {\n  color: red;\n}";
    let d =
      configured(&NoInvalidPositionDeclaration, None).check_source(&context(source, Syntax::Css));
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "Invalid position for declaration");
    assert_eq!(&source[d[0].span.offset..d[0].span.end()], "color: red;");
  }

  #[test]
  fn allows_style_rules_inside_media() {
    let source = "@media (min-width: 768px) { a { color: red; } }";
    assert!(lines(source, Syntax::Css, None).is_empty());
  }

  #[test]
  fn allows_scss_variable_inside_media() {
    let source = "@media (min-width: 768px) { $sidebar-width: 1px; }";
    assert!(lines(source, Syntax::Scss, None).is_empty());
  }

  #[test]
  fn allows_declarations_inside_font_face() {
    let source = "@font-face { font-family: x; }";
    assert!(lines(source, Syntax::Css, None).is_empty());
  }

  #[test]
  fn allows_nested_media_with_declarations() {
    let source = ".a { @media (min-width: 1px) { font-size: 1px; } }";
    assert!(lines(source, Syntax::Css, None).is_empty());
    assert!(lines(source, Syntax::Scss, None).is_empty());
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
    assert_eq!(lines("color: red;\n.a {}", Syntax::Css, None), vec![1]);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreAtRules": ["include", "/^mix/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Invalid option value \"always\" for rule \"no-invalid-position-declaration\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignoreAtRules": [1] }])),
      [
        "Invalid value \"1\" for option \"ignoreAtRules\" of rule \"no-invalid-position-declaration\""
      ]
    );
  }
}
