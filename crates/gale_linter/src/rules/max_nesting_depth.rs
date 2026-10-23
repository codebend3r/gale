use gale_css_parser::postcss_tree::{NodeKind, NodeRef, PostcssTree, RuleRef};
use gale_css_parser::selector::postcss::rule_selectors;
use gale_diagnostics::{Diagnostic, Span};

use crate::js_number;
use crate::lint_env::LintEnv;
use crate::options::{Accept, InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "max-nesting-depth";

declare_rule! {
  /// Limit nesting depth of CSS rules.
  ///
  /// Equivalent to Stylelint's `max-nesting-depth` rule.
  /// Default maximum: 3.
  ///
  /// ## Options
  ///
  /// Primary option: `<number>` — maximum allowed nesting depth.
  ///
  /// Secondary options:
  /// - `ignoreAtRules: [<string>, ...]` — at-rules whose name matches are
  ///   completely transparent: they do not count toward nesting, and all content
  ///   inside them is also exempt from depth checking.
  /// - `ignore: [<string>, ...]` — a list of selectors/at-rules to ignore:
  ///   - `"blockless-at-rules"` — at-rules that wrap content (like `@media`)
  ///     don't count toward depth.
  ///   - `"pseudo-classes"` — nested selectors that are pseudo-classes (e.g.,
  ///     `&:hover`) don't count toward depth.
  pub struct MaxNestingDepth => MaxNestingDepthCheck {
    name: NAME,
    description: "Limit nesting depth",
    default_severity: Warning,
  }
}

/// `max-nesting-depth` as configured.
pub struct MaxNestingDepthCheck {
  /// The deepest nesting allowed.
  max: f64,
  /// `ignoreAtRules`: at-rules, by name, that are transparent.
  ignore_at_rules: Vec<String>,
  /// `ignore: ["blockless-at-rules"]`.
  ignore_blockless_at_rules: bool,
  /// `ignore: ["pseudo-classes"]`.
  ignore_pseudo_classes: bool,
}

/// The depth allowed when the config gives none.
const MAX_DEPTH: f64 = 3.0;

impl MaxNestingDepthCheck {
  /// Stylelint's options: a number, then `ignore`, `ignoreAtRules`,
  /// `ignoreRules` and `ignorePseudoClasses`.  Gale reads no number as 3,
  /// and accepts but does not yet apply `ignoreRules` and
  /// `ignorePseudoClasses`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max = options.primary_number(&[Accept::Number]);
    let ignore = options.keywords("ignore", &["blockless-at-rules", "pseudo-classes"]);
    let ignore_at_rules = options.strings("ignoreAtRules");
    options.strings("ignoreRules");
    options.strings("ignorePseudoClasses");
    options.finish()?;
    Ok(Self {
      max: max.unwrap_or(MAX_DEPTH),
      ignore_at_rules,
      ignore_blockless_at_rules: ignore.contains(&"blockless-at-rules"),
      ignore_pseudo_classes: ignore.contains(&"pseudo-classes"),
    })
  }

  /// The problem for a node `depth` deep.
  fn problem(&self, depth: usize, span: Span) -> Diagnostic {
    Diagnostic::new(
      NAME,
      format!(
        "Expected nesting depth to be no more than {}, found {depth}",
        js_number::to_js_string(self.max)
      ),
    )
    .span(span)
  }

  /// Whether `depth` is deeper than allowed.
  fn too_deep(&self, depth: usize) -> bool {
    depth as f64 > self.max
  }
}

impl Check for MaxNestingDepthCheck {
  /// Flags every rule and at-rule with a block nested deeper than allowed,
  /// counting depth the way Stylelint's `nestingDepth` does.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    tree
      .walk()
      .filter(|node| node.has_block())
      .filter(|node| match node.kind {
        NodeKind::Rule => node.as_rule().is_some_and(RuleRef::is_standard_syntax),
        NodeKind::AtRule => !self.is_ignored_at_rule(*node),
        _ => false,
      })
      .filter_map(|node| {
        let depth = self.nesting_depth(node, 0);
        self
          .too_deep(depth)
          .then(|| self.problem(depth, node.span()))
      })
      .collect()
  }
}

impl MaxNestingDepthCheck {
  /// Whether `node` is an at-rule `ignoreAtRules` names.
  fn is_ignored_at_rule(&self, node: NodeRef) -> bool {
    node.as_at_rule().is_some_and(|at| {
      self
        .ignore_at_rules
        .iter()
        .any(|name| name.eq_ignore_ascii_case(at.name()))
    })
  }

  /// Stylelint's `nestingDepth`: `level` plus the nesting between `node`
  /// and the top level.  A rule directly in a top-level at-rule counts as
  /// top level, an ignored at-rule resets the count, and the nodes the
  /// `ignore` option names do not add to it.
  fn nesting_depth(&self, node: NodeRef, level: usize) -> usize {
    let Some(parent) = node.parent() else {
      return level;
    };
    if self.is_ignored_at_rule(parent) {
      return 0;
    }
    if parent.as_at_rule().is_some() && parent.parent().is_none() {
      return level;
    }
    let transparent = match node.kind {
      NodeKind::AtRule => {
        self.ignore_blockless_at_rules && node.children().all(|child| child.as_decl().is_none())
      }
      NodeKind::Rule => {
        self.ignore_pseudo_classes
          && node
            .as_rule()
            .is_some_and(|rule| contains_pseudo_classes_only(rule.selector()))
      }
      _ => false,
    };
    self.nesting_depth(parent, if transparent { level } else { level + 1 })
  }
}

/// Whether every selector in `selector` is a nested pseudo-class (`&:hover`
/// but not `&::before`), as Stylelint's `containsPseudoClassesOnly` decides.
fn contains_pseudo_classes_only(selector: &str) -> bool {
  rule_selectors(selector)
    .iter()
    .all(|part| part.starts_with("&:") && !part.starts_with("&::"))
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// The depths reported for `source` in `syntax` with `options`, with the
  /// text each report starts with.
  fn depths(source: &str, syntax: Syntax, options: serde_json::Value) -> Vec<(String, &str)> {
    configured(&MaxNestingDepth, options)
      .check_source(&context(source, syntax))
      .into_iter()
      .map(|d| {
        let found = d.message.rsplit(' ').next().unwrap().to_string();
        let text = &source[d.span.offset..d.span.offset + d.span.length];
        (found, text.split(' ').next().unwrap())
      })
      .collect()
  }

  /// `.level-N { ... }` nested `depth` deep around a `.leaf`.
  fn nested(depth: usize) -> String {
    let mut source = ".leaf { color: red; }".to_string();
    for level in 1..=depth {
      source = format!(".level-{level} {{ color: red; {source} }}");
    }
    source
  }

  #[test]
  fn reports_deep_nesting() {
    let source = nested(5);
    assert_eq!(
      depths(&source, Syntax::Scss, serde_json::json!(3)),
      [("4".to_string(), ".level-1"), ("5".to_string(), ".leaf")]
    );
  }

  #[test]
  fn allows_shallow_nesting() {
    assert!(depths(&nested(2), Syntax::Scss, serde_json::json!(3)).is_empty());
  }

  #[test]
  fn nested_at_rules_count_and_are_checked() {
    let source = "a { @media print { b { @include x { c {} } } } }";
    assert_eq!(
      depths(source, Syntax::Scss, serde_json::json!(2)),
      [("3".to_string(), "@include"), ("4".to_string(), "c")]
    );
    // A rule directly in a top-level at-rule is at the top level.
    assert!(
      depths(
        "@media print { a { b {} } }",
        Syntax::Css,
        serde_json::json!(1)
      )
      .is_empty()
    );
  }

  #[test]
  fn non_standard_rules_count_but_are_not_reported() {
    let source = "a { #{$b} { c {} } }";
    assert_eq!(
      depths(source, Syntax::Scss, serde_json::json!(1)),
      [("2".to_string(), "c")]
    );
  }

  #[test]
  fn ignore_at_rules_skips_all_content() {
    let options = serde_json::json!([0, { "ignoreAtRules": ["mixin"] }]);
    let source = "@mixin m { .a { .b { .c { .d { .e { color: red; } } } } } }";
    assert!(depths(source, Syntax::Scss, options).is_empty());
  }

  #[test]
  fn ignore_at_rules_still_flags_outside() {
    let options = serde_json::json!([0, { "ignoreAtRules": ["mixin"] }]);
    let source = ".top { .nested { color: red; } }";
    assert_eq!(depths(source, Syntax::Scss, options).len(), 1);
  }

  #[test]
  fn ignore_blockless_at_rules() {
    // `@media` holds no declarations, so it does not count.
    let source = ".a { .b { @media print { .c { color: red; } } } }";
    let options = serde_json::json!([2, { "ignore": ["blockless-at-rules"] }]);
    assert!(depths(source, Syntax::Scss, options).is_empty());
    assert_eq!(
      depths(source, Syntax::Scss, serde_json::json!(2)),
      [("3".to_string(), ".c")]
    );
  }

  #[test]
  fn ignore_pseudo_classes() {
    let source = ".a { &:hover, &:focus { .b { color: red; } } &::before { .c {} } }";
    let options = serde_json::json!([1, { "ignore": ["pseudo-classes"] }]);
    assert_eq!(
      depths(source, Syntax::Scss, options),
      [("2".to_string(), ".c")]
    );
  }

  #[test]

  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([2, {
          "ignore": ["pseudo-classes"],
          "ignoreAtRules": ["/^my-/"],
          "ignoreRules": [".a"],
          "ignorePseudoClasses": ["hover"]
        }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("deep")),
      ["Invalid option value \"deep\" for rule \"max-nesting-depth\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([2, { "ignore": ["rules"] }])),
      ["Invalid value \"rules\" for option \"ignore\" of rule \"max-nesting-depth\""]
    );
  }
}
