use std::collections::HashSet;

use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::value_parser::{self, ValueNode};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::standard_syntax::is_standard_syntax_value;

const NAME: &str = "no-unknown-animations";

declare_rule! {
  /// Warn when an `animation-name` or `animation` shorthand references a name
  /// that doesn't have a corresponding `@keyframes` definition in the same file.
  ///
  /// Equivalent to Stylelint's `no-unknown-animations` rule.
  /// Detection-only (no autofix).
  pub struct NoUnknownAnimations => NoUnknownAnimationsCheck {
    name: NAME,
    description: "Disallow unknown animation names",
    default_severity: Warning,
  }
}

/// `no-unknown-animations`, which takes no options.
pub struct NoUnknownAnimationsCheck;

impl NoUnknownAnimationsCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for NoUnknownAnimationsCheck {
  /// Collects the `@keyframes` names in the file, then flags each animation
  /// name in an `animation` or `animation-name` value that none of them
  /// define, as Stylelint does.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let declared: HashSet<&str> = tree
      .walk_at_rules()
      .filter(|at| at.is_keyframes())
      .map(|at| at.params())
      .collect();
    let mut diags = Vec::new();
    for decl in tree.walk_decls() {
      let prop = decl.prop();
      if !(prop.eq_ignore_ascii_case("animation") || prop.eq_ignore_ascii_case("animation-name")) {
        continue;
      }
      for name in find_animation_names(decl.value()) {
        if ANIMATION_NAME_KEYWORDS.contains(&name.value.to_ascii_lowercase().as_str())
          || declared.contains(name.value)
        {
          continue;
        }
        let start = decl.value_start();
        diags.push(
          Diagnostic::new(NAME, format!("Unknown animation name \"{}\"", name.value)).span(
            Span::from_range(start + name.source_index, start + name.source_end_index),
          ),
        );
      }
    }
    diags
  }
}

/// The CSS-wide keywords (Stylelint's `basicKeywords`).
const BASIC_KEYWORDS: &[&str] = &["initial", "inherit", "revert", "revert-layer", "unset"];

/// Words that are no animation name (Stylelint's `animationNameKeywords`).
const ANIMATION_NAME_KEYWORDS: &[&str] = &[
  "initial",
  "inherit",
  "revert",
  "revert-layer",
  "unset",
  "none",
];

/// The keywords of the other parts of the `animation` shorthand
/// (Stylelint's `animationShorthandKeywords`).
const ANIMATION_SHORTHAND_KEYWORDS: &[&str] = &[
  "initial",
  "inherit",
  "revert",
  "revert-layer",
  "unset",
  "none",
  "linear",
  "ease",
  "ease-in",
  "ease-in-out",
  "ease-out",
  "step-start",
  "step-end",
  "steps",
  "cubic-bezier",
  "infinite",
  "normal",
  "reverse",
  "alternate",
  "alternate-reverse",
  "forwards",
  "backwards",
  "both",
  "running",
  "paused",
];

/// Stylelint's `findAnimationName`: the words of an `animation` or
/// `animation-name` value that may name an animation; none when the value
/// holds preprocessor syntax.
fn find_animation_names(value: &str) -> Vec<ValueNode<'_>> {
  let nodes = value_parser::parse(value);
  // `inherit`, `initial` and the like.
  if let [only] = nodes.as_slice()
    && BASIC_KEYWORDS.contains(&only.value.to_ascii_lowercase().as_str())
  {
    return nodes;
  }
  let mut names = Vec::new();
  let mut ignored = false;
  value_parser::walk(&nodes, &mut |node| {
    if ignored {
      return false;
    }
    if node.is_function() {
      return false;
    }
    if !node.is_word() {
      return true;
    }
    let lower = node.value.to_ascii_lowercase();
    // A shorthand with preprocessor syntax names nothing that can be found.
    if !is_standard_syntax_value(&lower) {
      ignored = true;
      names.clear();
      return false;
    }
    if lower.starts_with("var(") || ANIMATION_SHORTHAND_KEYWORDS.contains(&lower.as_str()) {
      return true;
    }
    // Numbers, with or without a unit (`getDimension`).
    if !lower.starts_with('#') {
      let hackless = node.value.replacen("\\0", "", 1).replacen("\\9", "", 1);
      if value_parser::unit(&hackless).is_some() {
        return true;
      }
    }
    names.push(node.clone());
    true
  });
  names
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context};

  /// What the rule reports for `source` in `syntax`: each message and the
  /// text it points at.
  fn check(source: &str, syntax: Syntax) -> Vec<(String, &str)> {
    configured(&NoUnknownAnimations, None)
      .check_source(&context(source, syntax))
      .into_iter()
      .map(|d| (d.message, &source[d.span.offset..d.span.end()]))
      .collect()
  }

  #[test]
  fn reports_unknown_animation_name() {
    assert_eq!(
      check("a { animation-name: fadeIn; }", Syntax::Css),
      [("Unknown animation name \"fadeIn\"".to_string(), "fadeIn")]
    );
  }

  #[test]
  fn allows_known_animation_name() {
    let source = "@keyframes fadeIn {}\na { animation-name: fadeIn, none; }";
    assert!(check(source, Syntax::Css).is_empty());
    let source = "@media print { @-webkit-keyframes spin {} }\na { animation: spin 1s; }";
    for syntax in [Syntax::Css, Syntax::Scss] {
      assert!(check(source, syntax).is_empty());
    }
  }

  #[test]
  fn reports_unknown_in_animation_shorthand() {
    assert_eq!(
      check("a { animation: slideUp 1s ease; }", Syntax::Css),
      [("Unknown animation name \"slideUp\"".to_string(), "slideUp")]
    );
    // Functions, numbers and keywords are other parts of the shorthand.
    let source = "a { animation: 1s cubic-bezier(0.1, 0.7, 1, 0.1) 2 infinite spin; }";
    assert_eq!(
      check(source, Syntax::Css),
      [("Unknown animation name \"spin\"".to_string(), "spin")]
    );
  }

  #[test]
  fn names_are_case_sensitive_and_each_is_reported() {
    let source = "@keyframes a {}\nb { animation-name: A, a, c; }";
    assert_eq!(
      check(source, Syntax::Css)
        .into_iter()
        .map(|(_, text)| text)
        .collect::<Vec<_>>(),
      ["A", "c"]
    );
  }

  #[test]
  fn preprocessor_values_name_nothing() {
    let source = "a { animation: $name 1s; animation-name: #{$x}; b { animation: @n 1s } }";
    assert!(check(source, Syntax::Scss).is_empty());
  }

  #[test]
  fn checks_declarations_wherever_they_are() {
    let source = "a { @media print { animation: gone 1s; } }";
    assert_eq!(check(source, Syntax::Scss).len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(invalid_options("no-unknown-animations", serde_json::json!(true)).is_empty());
    // The rule takes nothing but `true`, and ignores secondary options.
    assert!(
      invalid_options(
        "no-unknown-animations",
        serde_json::json!([true, { "foo": 1 }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options("no-unknown-animations", serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"no-unknown-animations\""]
    );
  }
}
