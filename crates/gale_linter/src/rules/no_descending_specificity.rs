use std::collections::HashMap;

use gale_css_parser::postcss_tree::{NodeRef, PostcssTree, RuleRef};
use gale_css_parser::selector::postcss::split_selectors;
use gale_css_parser::selector::specificity::{self, Specificity};
use gale_css_parser::selector::{Selector, parse_selector_list};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::nested_selectors::{SelectorResolver, last_compound_without_pseudo_classes};
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "no-descending-specificity";

declare_rule! {
  /// Reports when a selector with lower specificity appears after a selector with
  /// higher specificity, which may indicate a specificity ordering issue.
  ///
  /// Equivalent to Stylelint's `no-descending-specificity` rule.
  pub struct NoDescendingSpecificity => NoDescendingSpecificityCheck {
    name: NAME,
    description: "Disallow selectors of lower specificity from coming after overriding selectors of higher specificity",
    default_severity: Warning,
  }
}

/// `no-descending-specificity` as configured.
pub struct NoDescendingSpecificityCheck {
  /// `ignore: ["selectors-within-list"]`: rules with more than one selector
  /// are not compared.
  ignore_selectors_within_list: bool,
}

impl NoDescendingSpecificityCheck {
  /// Stylelint's options: `true`, then `ignore: ["selectors-within-list"]`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore = options.keywords("ignore", &["selectors-within-list"]);
    options.finish()?;
    Ok(Self {
      ignore_selectors_within_list: ignore.contains(&"selectors-within-list"),
    })
  }
}

/// A selector checked earlier in a scope.
struct Entry {
  /// Its specificity.
  specificity: Specificity,
  /// The selector resolved against its ancestors, as the message prints it.
  resolved: String,
}

/// The comparison context of one scope: the selectors seen so far, by their
/// reference selector (the last compound without pseudo-classes).
type Context = HashMap<String, Vec<Entry>>;

/// The walk over the rules, resolving each one's selectors against its
/// ancestors as Stylelint's `resolveNestedSelectorsForRule` does.
struct Walk<'t> {
  /// The file's text.
  source: &'t str,
  /// `ignore: ["selectors-within-list"]`.
  ignore_selectors_within_list: bool,
  /// Each rule's selectors resolved against its ancestors.
  resolver: SelectorResolver,
  /// One comparison context per scope: the root (`None`) and each at-rule.
  contexts: HashMap<Option<usize>, Context>,
  /// What the walk has found.
  diagnostics: Vec<Diagnostic>,
}

impl<'t> Walk<'t> {
  /// Check `rule`, as Stylelint's `walkRules` callback does.
  fn rule(&mut self, rule: RuleRef<'t>) {
    // Nested properties, Less mixins and interpolated selectors.
    if !rule.is_standard_syntax() {
      return;
    }
    if self.ignore_selectors_within_list && split_selectors(rule.selector()).len() > 1 {
      return;
    }
    // Specificity only matters to declarations.
    if !has_declaration(rule.node()) {
      return;
    }
    let raw = rule.raw_selector();
    let Ok(own) = parse_selector_list(&raw) else {
      return;
    };
    let Some(resolved) = self.resolver.resolve(rule) else {
      return;
    };
    // Comparisons are scoped to the nearest at-rule.
    let scope = rule
      .ancestors()
      .find(|node| node.as_at_rule().is_some())
      .map(NodeRef::id);
    for (selector, resolved) in own.selectors.iter().zip(&resolved.selectors) {
      self.check_selector(rule, &raw, selector, resolved, scope);
    }
  }

  /// Compare one selector of `rule` (`selector` of its selector text `raw`,
  /// `resolved` against its ancestors) with those seen before it in
  /// `scope`.
  fn check_selector(
    &mut self,
    rule: RuleRef,
    raw: &str,
    selector: &Selector,
    resolved: &Selector,
    scope: Option<usize>,
  ) {
    let Some(reference) = last_compound_without_pseudo_classes(resolved) else {
      return;
    };
    let resolved_text = resolved.to_string();
    let Some(specificity) = specificity::of_text(&resolved_text) else {
      return;
    };
    let written = raw.get(selector.offset..selector.end).unwrap_or("").trim();
    let span = self.selector_span(rule, raw, selector, written);
    let entries = self
      .contexts
      .entry(scope)
      .or_default()
      .entry(reference)
      .or_default();
    if let Some(prior) = entries.iter().find(|prior| specificity < prior.specificity) {
      self.diagnostics.push(
        Diagnostic::new(
          NAME,
          format!(
            "Expected selector \"{written}\" to come before selector \"{}\"",
            prior.resolved
          ),
        )
        .span(span),
      );
    }
    entries.push(Entry {
      specificity,
      resolved: resolved_text,
    });
  }

  /// Where `selector`, written `written`, sits in the source.  `raw` is
  /// the source text of the rule's selector unless SCSS `//` comments in it
  /// were rewritten, in which case the selector is looked up in the source.
  fn selector_span(&self, rule: RuleRef, raw: &str, selector: &Selector, written: &str) -> Span {
    let start = rule.selector_start();
    let source = self.source.get(start..rule.name_span.end).unwrap_or("");
    let offset = if source == raw {
      selector.offset
    } else {
      source.find(written).unwrap_or(0)
    };
    Span::new(start + offset, written.len())
  }
}

/// Stylelint's `hasDeclaration`: whether `node` holds a declaration,
/// directly or in an at-rule inside it.
fn has_declaration(node: NodeRef) -> bool {
  node.children().any(|child| {
    child.as_decl().is_some() || (child.as_at_rule().is_some() && has_declaration(child))
  })
}

impl Check for NoDescendingSpecificityCheck {
  /// Groups selectors by their last compound and flags one whose
  /// specificity is lower than an earlier one's in the same scope, nested
  /// selectors resolved against their parents as CSS nesting does.
  fn check(&self, tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    let mut walk = Walk {
      source: context.source,
      ignore_selectors_within_list: self.ignore_selectors_within_list,
      resolver: SelectorResolver::default(),
      contexts: HashMap::new(),
      diagnostics: Vec::new(),
    };
    for rule in tree.walk_rules() {
      walk.rule(rule);
    }
    walk.diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context};

  /// What the rule reports for `source` in `syntax`: each message and the
  /// text it points at.
  fn check(source: &str, syntax: Syntax) -> Vec<(String, &str)> {
    configured(&NoDescendingSpecificity, None)
      .check_source(&context(source, syntax))
      .into_iter()
      .map(|d| {
        (
          d.message,
          &source[d.span.offset..d.span.offset + d.span.length],
        )
      })
      .collect()
  }

  /// The texts the rule reports for `source` in `syntax`.
  fn texts(source: &str, syntax: Syntax) -> Vec<&str> {
    check(source, syntax)
      .into_iter()
      .map(|(_, text)| text)
      .collect()
  }

  #[test]
  fn reports_descending_specificity() {
    // `.foo .bar` then `.bar`: the same last compound (`.bar`), descending.
    let source = ".foo .bar { color: red; }\n.bar { color: blue; }";
    for syntax in [Syntax::Css, Syntax::Scss, Syntax::Less] {
      assert_eq!(
        check(source, syntax),
        [(
          "Expected selector \".bar\" to come before selector \".foo .bar\"".to_string(),
          ".bar"
        )]
      );
    }
  }

  #[test]
  fn reports_each_selector_of_a_list_where_it_is_written() {
    let source = "#a b, #a c { top: 0 }\nb, /* x, y */\n  c { top: 1px }";
    assert_eq!(texts(source, Syntax::Css), ["b", "c"]);
    // SCSS line comments are rewritten in the selector text, so the
    // selector is found in the source.
    let source = "#a b { top: 0 }\nc, // b, d\nb { top: 0 }";
    assert_eq!(texts(source, Syntax::Scss), ["b"]);
  }

  #[test]
  fn no_report_for_different_last_compound() {
    assert!(check("#id { top: 0 }\na { top: 0 }", Syntax::Css).is_empty());
  }

  #[test]
  fn reference_selectors_keep_pseudo_elements_and_ignore_order() {
    // `.x::after` targets the pseudo-element, not `.x`, written with one
    // colon or two.
    for source in [
      ".x::after { content: 'a'; }\n.x { color: red; }",
      ".y .x:after { content: 'a'; }\n.x { color: red; }",
    ] {
      assert!(check(source, Syntax::Css).is_empty(), "{source}");
    }
    let key = |text: &str| {
      let list = parse_selector_list(text).unwrap();
      last_compound_without_pseudo_classes(&list.selectors[0])
    };
    assert_eq!(key("a > .b.a:hover"), Some(".a.b".to_string()));
    assert_eq!(key(".x:hover::before"), Some(".x::before".to_string()));
    assert_eq!(key(":hover"), None);
  }

  #[test]
  fn ignores_ascending_specificity() {
    assert!(check("a { top: 0 }\n.cls { top: 0 }\n#id { top: 0 }", Syntax::Css).is_empty());
  }

  #[test]
  fn skips_rules_without_declarations() {
    assert!(check(".foo .bar { }\n.bar { color: blue; }", Syntax::Css).is_empty());
    // A declaration in an at-rule inside the rule counts.
    let source = ".foo .bar { @media print { color: red } }\n.bar { color: blue; }";
    assert_eq!(check(source, Syntax::Scss).len(), 1);
  }

  #[test]
  fn nested_selectors_resolve_as_css_nesting_does() {
    // The nested `.bar` is `.foo .bar .bar`, which is not descending.
    let source = ".foo .bar { .bar { color: red; } }";
    for syntax in [Syntax::Css, Syntax::Scss] {
      assert!(check(source, syntax).is_empty());
    }
    // `&:hover` resolves to `.x:hover`, whose reference is `.x`.
    let source = ".x { &:hover { top: 0 } }\n.x { top: 0 }";
    assert_eq!(
      check(source, Syntax::Scss),
      [(
        "Expected selector \".x\" to come before selector \".x:hover\"".to_string(),
        ".x"
      )]
    );
    // A parent that is not one compound selector becomes `:is()`, which is
    // dropped from the reference with the pseudo-classes.
    for source in [
      "#a b { &:hover { top: 0 } }\nb { top: 0 }",
      "#a b, #c b { &:hover { top: 0 } }\nb { top: 0 }",
    ] {
      assert!(check(source, Syntax::Css).is_empty(), "{source}");
    }
  }

  #[test]
  fn at_rules_scope_the_comparison() {
    let source = "#a b { top: 0 }\n@media print { b { top: 0 } }";
    assert!(check(source, Syntax::Css).is_empty());
    // A rule nested in an at-rule in a rule is in the at-rule's scope.
    let source = "a { @media print { #a b { top: 0 } } }\n@media print { b { top: 0 } }";
    assert!(check(source, Syntax::Scss).is_empty());
  }

  #[test]
  fn skips_selectors_that_are_not_standard_syntax() {
    assert!(check("#id { top: 0 }\n.#{$var} { top: 0 }", Syntax::Scss).is_empty());
    assert!(check("#id { top: 0 }\n%placeholder { top: 0 }", Syntax::Scss).is_empty());
    // Nor are the rules inside one resolved.
    let source = "#a b { top: 0 }\n.#{$x} { b { top: 0 } }";
    assert!(check(source, Syntax::Scss).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignore": ["selectors-within-list"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"no-descending-specificity\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignore": ["lists"] }])),
      ["Invalid value \"lists\" for option \"ignore\" of rule \"no-descending-specificity\""]
    );
  }

  #[test]
  fn ignore_selectors_within_list_skips_rules_with_several_selectors() {
    use crate::testing::lint;
    let source = "#a b { color: red; }\nb, c { color: blue; }";
    for syntax in [Syntax::Css, Syntax::Scss] {
      assert_eq!(lint(NAME, serde_json::json!(true), source, syntax).len(), 1);
      let ignore = serde_json::json!([true, { "ignore": ["selectors-within-list"] }]);
      assert!(lint(NAME, ignore, source, syntax).is_empty());
    }
  }
}
