use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::data::{is_experimental_html_element, is_known_html_element};
use crate::rule::{Rule, RuleContext};
use crate::selector::nesting::{resolve_nested, resolve_nested_list};
use crate::selector::{
  Pseudo, Selector, SelectorList, SelectorNode, is_standard_syntax_selector, parse_selector_list,
  walk_pseudos,
};
use crate::style_rules::{RawStyleRule, scan_style_rules};

/// Disallow unmatchable selectors.
///
/// Equivalent to Stylelint's `selector-no-unmatchable` rule. Unmatchable
/// selectors are valid but can never match anything. Nested selectors are
/// resolved according to the CSS Nesting specification before checking.
pub struct SelectorNoUnmatchable;

/// Pseudo-classes that take a forgiving selector list.
const FORGIVING_PSEUDO_CLASSES: &[&str] = &["is", "where"];

/// Pseudo-classes whose argument flips or relates matching, so unmatchable
/// arguments inside them are not unmatchable selectors.
const NEGATION_AND_RELATIONAL_PSEUDO_CLASSES: &[&str] = &["has", "not"];

/// Pseudo-elements that represent real elements; see
/// https://drafts.csswg.org/css-pseudo/#element-backed and
/// https://drafts.csswg.org/css-shadow/#slotted-pseudo
const ELEMENT_REPRESENTING_PSEUDO_ELEMENTS: &[&str] =
  &["details-content", "file-selector-button", "part", "slotted"];

/// Tree-structural pseudo-classes; see
/// https://drafts.csswg.org/selectors/#structural-pseudos
const TREE_STRUCTURAL_PSEUDO_CLASSES: &[&str] = &[
  "empty",
  "first-child",
  "first-of-type",
  "last-child",
  "last-of-type",
  "nth-child",
  "nth-last-child",
  "nth-last-of-type",
  "nth-of-type",
  "only-child",
  "only-of-type",
  "root",
];

const DISABLEABLE_ELEMENTS: &[&str] = &[
  "button", "fieldset", "input", "optgroup", "option", "select", "textarea",
];
const CHECKABLE_ELEMENTS: &[&str] = &["input", "option"];
const REQUIRABLE_ELEMENTS: &[&str] = &["input", "select", "textarea"];
const VALIDATABLE_ELEMENTS: &[&str] =
  &["button", "fieldset", "form", "input", "select", "textarea"];
const RANGEABLE_ELEMENTS: &[&str] = &["input"];
const PLACEHOLDER_SHOWABLE_ELEMENTS: &[&str] = &["input", "textarea"];
const DEFAULTABLE_ELEMENTS: &[&str] = &["button", "input", "option"];
const AUTOFILLABLE_ELEMENTS: &[&str] = &["input", "select", "textarea"];
const LINK_ELEMENTS: &[&str] = &["a", "area"];

/// The elements a pseudo-class can match, for pseudo-classes limited to
/// specific elements; see
/// https://html.spec.whatwg.org/multipage/semantics-other.html#pseudo-classes
fn applicable_elements(pseudo_class: &str) -> Option<&'static [&'static str]> {
  Some(match pseudo_class {
    "any-link" | "link" | "visited" => LINK_ELEMENTS,
    "autofill" => AUTOFILLABLE_ELEMENTS,
    "checked" => CHECKABLE_ELEMENTS,
    "default" => DEFAULTABLE_ELEMENTS,
    "disabled" | "enabled" => DISABLEABLE_ELEMENTS,
    "in-range" | "out-of-range" => RANGEABLE_ELEMENTS,
    "invalid" | "user-invalid" | "user-valid" | "valid" => VALIDATABLE_ELEMENTS,
    "optional" | "required" => REQUIRABLE_ELEMENTS,
    "placeholder-shown" => PLACEHOLDER_SHOWABLE_ELEMENTS,
    _ => return None,
  })
}

impl Rule for SelectorNoUnmatchable {
  fn name(&self) -> &'static str {
    "selector-no-unmatchable"
  }

  fn description(&self) -> &'static str {
    "Disallow unmatchable selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Checks every style rule's selectors, resolving nested selectors against
  /// their ancestors first.
  ///
  /// Preludes are read from the source text rather than the parsed AST so
  /// that selectors are seen exactly as written.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let rules = scan_style_rules(ctx.source, ctx.syntax);
    let mut diagnostics = Vec::new();
    // Each rule's resolved selector list, for its nested rules to build on.
    // Parents precede their children, so one pass in document order does.
    let mut resolved: Vec<Option<SelectorList>> = Vec::with_capacity(rules.len());
    for raw in &rules {
      let entry = self.check_rule(raw, &resolved, &mut diagnostics);
      resolved.push(entry);
    }
    diagnostics
  }
}

impl SelectorNoUnmatchable {
  /// Checks one rule's selectors and returns them resolved against the
  /// parent, or `None` when the rule or an ancestor cannot be resolved.
  fn check_rule(
    &self,
    raw: &RawStyleRule,
    resolved: &[Option<SelectorList>],
    diagnostics: &mut Vec<Diagnostic>,
  ) -> Option<SelectorList> {
    if !is_standard_syntax_selector(&raw.prelude) {
      return None;
    }
    let own = parse_selector_list(&raw.prelude).ok()?;
    let parent = match raw.parent {
      Some(index) => Some(resolved[index].as_ref()?),
      None => None,
    };

    for selector in &own.selectors {
      let nested = parent.map(|parent| resolve_nested(selector, parent));
      let subject = nested.as_ref().unwrap_or(selector);
      let reason = check_unrepresentable_pseudo_elements(selector, nested.as_ref())
        .or_else(|| check_shadow(subject))
        .or_else(|| check_pseudo_classing_pseudo_elements(subject))
        .or_else(|| check_applicable_elements(subject));
      let Some(reason) = reason else {
        continue;
      };

      let text = &raw.prelude[selector.offset..selector.end];
      let written = match &nested {
        Some(nested) => format!("\"{text}\" (\"{}\")", nested.to_string().trim()),
        None => format!("\"{text}\""),
      };
      diagnostics.push(
        Diagnostic::new(
          self.name(),
          format!("Unmatchable selector {written}, {reason}"),
        )
        .severity(self.default_severity())
        .span(Span::from_range(
          raw.offset + selector.offset,
          raw.offset + selector.end,
        )),
      );
    }

    Some(match parent {
      Some(parent) => resolve_nested_list(&own, parent),
      None => own,
    })
  }
}

fn is_negation_or_relational(pseudo: &Pseudo) -> bool {
  pseudo.is_one_of(NEGATION_AND_RELATIONAL_PSEUDO_CLASSES)
}

// ---------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------

/// Pseudo-elements cannot be represented by `:is()`, `:where()` or `&`; see
/// https://drafts.csswg.org/selectors/#matches
fn check_unrepresentable_pseudo_elements(
  selector: &Selector,
  nested: Option<&Selector>,
) -> Option<String> {
  let mut reason = None;
  walk_pseudos(&selector.nodes, &mut |visit| {
    if !visit.pseudo.element
      || visit
        .ancestors
        .iter()
        .any(|ancestor| is_negation_or_relational(ancestor))
    {
      return true;
    }
    let Some(nearest) = visit.ancestors.last() else {
      return true;
    };
    if nearest.is_one_of(FORGIVING_PSEUDO_CLASSES) {
      reason = Some(format!(
        "pseudo-elements cannot be represented by \":{}()\"",
        nearest.lower_name()
      ));
      return false;
    }
    true
  });
  reason.or_else(|| nested.and_then(find_unrepresentable_nesting_selector))
}

/// The nesting selector cannot represent pseudo-elements; see
/// https://drafts.csswg.org/css-nesting/#nest-selector
fn find_unrepresentable_nesting_selector(resolved: &Selector) -> Option<String> {
  let mut found = false;
  walk_pseudos(&resolved.nodes, &mut |visit| {
    let pseudo = visit.pseudo;
    found = !pseudo.element
      && pseudo.is_one_of(FORGIVING_PSEUDO_CLASSES)
      && !visit
        .ancestors
        .iter()
        .any(|ancestor| is_negation_or_relational(ancestor))
      && pseudo
        .selectors()
        .is_some_and(|list| list.any_node(SelectorNode::is_pseudo_element));
    !found
  });
  found.then(|| "\"&\" cannot represent pseudo-elements".to_string())
}

/// The shadow host is featureless and has no selectable ancestors or
/// siblings; see https://drafts.csswg.org/css-shadow-1/#host-selector
fn check_shadow(selector: &Selector) -> Option<String> {
  let mut reason = None;
  walk_pseudos(&selector.nodes, &mut |visit| {
    let pseudo = visit.pseudo;

    if pseudo.element && pseudo.is("slotted") {
      let slots_host = pseudo.selectors().is_some_and(|list| {
        list.any_node(|node| {
          node
            .as_pseudo()
            .is_some_and(|inner| !inner.element && inner.is("host"))
        })
      });
      if slots_host {
        reason = Some("slotted elements are never the shadow host".to_string());
      }
      return false;
    }

    if pseudo.element
      || !pseudo.is("host")
      || visit
        .ancestors
        .iter()
        .any(|ancestor| is_negation_or_relational(ancestor))
    {
      return true;
    }

    if visit.siblings[..visit.index]
      .iter()
      .any(SelectorNode::is_combinator)
    {
      reason = Some("the shadow host has no ancestors or siblings in its shadow tree".to_string());
      return false;
    }

    if let Some(feature) = compound_feature_sibling(visit.siblings, visit.index) {
      reason = Some(format!("\"{feature}\" never matches the shadow host"));
      return false;
    }

    true
  });
  reason
}

/// A type, class, id or attribute selector compounded with the node at
/// `index`, if any.
fn compound_feature_sibling(siblings: &[SelectorNode], index: usize) -> Option<&SelectorNode> {
  let before = siblings[..index]
    .iter()
    .enumerate()
    .rev()
    .take_while(|(_, node)| !node.is_combinator());
  let after = siblings
    .iter()
    .enumerate()
    .skip(index + 1)
    .take_while(|(_, node)| !node.is_combinator() && !node.is_pseudo_element());
  before
    .chain(after)
    .find(|(i, node)| is_feature_selector(i.checked_sub(1).map(|j| &siblings[j]), node))
    .map(|(_, node)| node)
}

/// Whether `node` is a type, class, id or attribute selector.
fn is_feature_selector(prev: Option<&SelectorNode>, node: &SelectorNode) -> bool {
  match node {
    SelectorNode::Tag(_) => !is_nesting_suffix(prev),
    SelectorNode::Class(_) | SelectorNode::Id(_) | SelectorNode::Attribute(_) => true,
    _ => false,
  }
}

/// Whether a type selector after `&` is really a nesting suffix, as in the
/// `-bar` of `&-bar`, rather than an element name.
fn is_nesting_suffix(prev: Option<&SelectorNode>) -> bool {
  matches!(prev, Some(SelectorNode::Nesting))
}

/// Tree-structural pseudo-classes never match pseudo-elements; see
/// https://drafts.csswg.org/selectors/#structural-pseudos
fn check_pseudo_classing_pseudo_elements(selector: &Selector) -> Option<String> {
  compound_selectors(&selector.nodes)
    .into_iter()
    .find_map(|compound| {
      let first = compound
        .first()?
        .as_pseudo()
        .filter(|pseudo| pseudo.element)?;
      if first.is_one_of(ELEMENT_REPRESENTING_PSEUDO_ELEMENTS) {
        return None;
      }
      let structural = compound.iter().find_map(|node| {
        node
          .as_pseudo()
          .filter(|pseudo| !pseudo.element && pseudo.is_one_of(TREE_STRUCTURAL_PSEUDO_CLASSES))
      })?;
      Some(format!(
        "\"{}\" never matches pseudo-elements",
        structural.value()
      ))
    })
}

/// Some pseudo-classes only match specific elements; see
/// https://html.spec.whatwg.org/multipage/semantics-other.html#pseudo-classes
fn check_applicable_elements(selector: &Selector) -> Option<String> {
  compound_selectors(&selector.nodes)
    .into_iter()
    .find_map(|compound| {
      if compound
        .first()
        .is_some_and(|node| node.is_pseudo_element())
      {
        return None;
      }

      let applicable: Vec<(&Pseudo, &[&str])> = compound
        .iter()
        .filter_map(|node| {
          let pseudo = node.as_pseudo().filter(|pseudo| !pseudo.element)?;
          Some((pseudo, applicable_elements(&pseudo.lower_name())?))
        })
        .collect();
      if applicable.is_empty() {
        return None;
      }

      // A type selector the pseudo-class never matches.
      if let Some((tag, element)) = html_type_selector(&compound)
        && let Some((pseudo, _)) = applicable
          .iter()
          .find(|(_, elements)| !elements.contains(&element.as_str()))
      {
        return Some(format!(
          "\"{}\" never matches \"{tag}\" elements",
          pseudo.value()
        ));
      }

      // Two pseudo-classes with no element in common.
      for (i, (first, first_elements)) in applicable.iter().enumerate() {
        for (second, second_elements) in &applicable[i + 1..] {
          if first_elements.iter().all(|e| !second_elements.contains(e)) {
            return Some(format!(
              "\"{}\" and \"{}\" never match the same element",
              first.value(),
              second.value()
            ));
          }
        }
      }
      None
    })
}

/// The first type selector in a compound that names an HTML element: its
/// name as written and lower-cased.
fn html_type_selector<'a>(compound: &[&'a SelectorNode]) -> Option<(&'a str, String)> {
  let (index, name) = compound
    .iter()
    .enumerate()
    .find_map(|(i, node)| match node {
      SelectorNode::Tag(name) => Some((i, name.as_str())),
      _ => None,
    })?;
  if name.contains('|') || is_nesting_suffix(index.checked_sub(1).map(|i| compound[i])) {
    return None;
  }
  let lower = name.to_ascii_lowercase();
  (is_known_html_element(&lower) || is_experimental_html_element(&lower)).then_some((name, lower))
}

// ---------------------------------------------------------------------------
// Compound selectors, following Stylelint's groupByCompoundSelectors with
// `groupNegationArguments: false`
// ---------------------------------------------------------------------------

type Compounds<'a> = Vec<Vec<&'a SelectorNode>>;

/// Every compound selector reachable from a selector, with `:has()`
/// arguments and forgiving selector lists expanded and comments dropped.
fn compound_selectors(nodes: &[SelectorNode]) -> Compounds<'_> {
  let (mut compounds, mut current) = group_compounds(nodes);
  compounds.append(&mut current);
  compounds
    .into_iter()
    .map(|compound| {
      compound
        .into_iter()
        .filter(|node| !matches!(node, SelectorNode::Comment(_)))
        .collect::<Vec<_>>()
    })
    .filter(|compound| !compound.is_empty())
    .collect()
}

/// Split `nodes` into the compounds ended by a combinator or pseudo-element
/// and the compounds still open at the end.
fn group_compounds(nodes: &[SelectorNode]) -> (Compounds<'_>, Compounds<'_>) {
  let mut terminated: Compounds = Vec::new();
  let mut current: Compounds = vec![Vec::new()];

  for node in nodes {
    if node.is_combinator() {
      terminated.append(&mut current);
      current = vec![Vec::new()];
      continue;
    }

    if node.is_pseudo_element() {
      terminated.append(&mut current);
      current = vec![Vec::new()];
    }

    if let SelectorNode::Pseudo(pseudo) = node
      && !pseudo.element
      && let Some(list) = pseudo.selectors()
    {
      if pseudo.is("has") && !list.selectors.is_empty() {
        for child in &list.selectors {
          let (mut child_terminated, mut child_current) = group_compounds(&child.nodes);
          terminated.append(&mut child_terminated);
          terminated.append(&mut child_current);
        }
        continue;
      }

      if !pseudo.is("not") {
        let mut combinations: Compounds = Vec::new();
        for child in &list.selectors {
          let (mut child_terminated, child_current) = group_compounds(&child.nodes);
          terminated.append(&mut child_terminated);
          for child_compound in child_current {
            if child_compound.is_empty() {
              continue;
            }
            for compound in &current {
              let mut combined = compound.clone();
              combined.extend(child_compound.iter().copied());
              combinations.push(combined);
            }
          }
        }
        if !combinations.is_empty() {
          current = combinations;
        }
        continue;
      }
    }

    for compound in &mut current {
      compound.push(node);
    }
  }

  (terminated, current)
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Syntax, parse};
  use gale_diagnostics::SourceLineIndex;

  /// (message, line, column, endLine, endColumn) for every warning, in order.
  fn lint_with(css: &str, syntax: Syntax) -> Vec<(String, usize, usize, usize, usize)> {
    let parsed = parse(css, syntax).expect("fixture should parse");
    let options = serde_json::json!([true]);
    let ctx = RuleContext {
      file_path: "t.css",
      source: css,
      syntax,
      options: Some(&options),
    };
    let index = SourceLineIndex::build(css);
    SelectorNoUnmatchable
      .check_root(&parsed.nodes, &ctx)
      .into_iter()
      .map(|d| {
        let (l, c) = index.offset_to_location(d.span.offset);
        let (el, ec) = index.offset_to_location(d.span.end());
        (d.message, l, c, el, ec)
      })
      .collect()
  }

  fn lint(css: &str) -> Vec<(String, usize, usize, usize, usize)> {
    lint_with(css, Syntax::Css)
  }

  fn rejected(selector: &str, resolved: &str, reason: &str) -> String {
    let subject = if resolved.is_empty() {
      format!("\"{selector}\"")
    } else {
      format!("\"{selector}\" (\"{resolved}\")")
    };
    format!("Unmatchable selector {subject}, {reason}")
  }

  #[test]
  fn accepts_matchable_selectors() {
    for css in [
      "a {}",
      "a:hover {}",
      "a::before {}",
      "a { .foo {} }",
      "a { &::before {} }",
      "@keyframes foo { to {} }",
      "::slotted(a):hover {}",
      "::slotted(a):first-child {}",
      "::before:hover {}",
      "a:first-child::before {}",
      ":host {}",
      ":host(.foo) {}",
      ":host:hover {}",
      ":host a {}",
      "*:host {}",
      "a:not(:host) {}",
      "input:checked {}",
      "input:required:valid {}",
      "custom-element:checked {}",
      "svg|path:checked {}",
      "a:not(:checked) {}",
      ".foo:has(::before) {}",
      ":not(::before) {}",
    ] {
      assert_eq!(lint(css), vec![], "should accept {css:?}");
    }
  }

  #[test]
  fn accepts_scss_specific_selectors() {
    for css in [
      "#{$foo}:enabled {}",
      "%foo:host {}",
      "a { #{&}::before {} }",
      "@mixin foo { &:first-child {} }",
    ] {
      assert_eq!(
        lint_with(css, Syntax::Scss),
        vec![],
        "should accept {css:?}"
      );
    }
  }

  #[test]
  fn rejects_pseudo_elements_within_is() {
    let reason = "pseudo-elements cannot be represented by \":is()\"";
    assert_eq!(
      lint(":is(::before) {}"),
      vec![(rejected(":is(::before)", "", reason), 1, 1, 1, 14)]
    );
    assert_eq!(
      lint(":is(.foo, ::before) {}"),
      vec![(rejected(":is(.foo, ::before)", "", reason), 1, 1, 1, 20)]
    );
    assert_eq!(
      lint(".foo:hover :is(a::before) {}"),
      vec![(
        rejected(".foo:hover :is(a::before)", "", reason),
        1,
        1,
        1,
        26
      )]
    );
    assert_eq!(
      lint(":is(a, a::before):hover {}"),
      vec![(rejected(":is(a, a::before):hover", "", reason), 1, 1, 1, 24)]
    );
  }

  #[test]
  fn rejects_pseudo_elements_within_where() {
    assert_eq!(
      lint(":where(input::placeholder) {}"),
      vec![(
        rejected(
          ":where(input::placeholder)",
          "",
          "pseudo-elements cannot be represented by \":where()\""
        ),
        1,
        1,
        1,
        27
      )]
    );
  }

  #[test]
  fn rejects_nesting_selectors_that_would_represent_pseudo_elements() {
    let reason = "\"&\" cannot represent pseudo-elements";
    assert_eq!(
      lint("a::before { .foo:hover & {} }"),
      vec![(
        rejected(".foo:hover &", ".foo:hover :is(a::before)", reason),
        1,
        13,
        1,
        25
      )]
    );
    assert_eq!(
      lint("a::before { &:hover {} }"),
      vec![(
        rejected("&:hover", ":is(a::before):hover", reason),
        1,
        13,
        1,
        20
      )]
    );
    assert_eq!(
      lint("a, a::before { &:hover {} }"),
      vec![(
        rejected("&:hover", ":is(a,a::before):hover", reason),
        1,
        16,
        1,
        23
      )]
    );
  }

  #[test]
  fn rejects_implicit_nesting_under_a_pseudo_element() {
    assert_eq!(
      lint("a::before { .foo {} }"),
      vec![(
        rejected(
          ".foo",
          ":is(a::before) .foo",
          "\"&\" cannot represent pseudo-elements"
        ),
        1,
        13,
        1,
        17
      )]
    );
  }

  #[test]
  fn resolves_nesting_before_checking_is() {
    assert_eq!(
      lint("a { &:is(::after) {} }"),
      vec![(
        rejected(
          "&:is(::after)",
          "a:is(::after)",
          "pseudo-elements cannot be represented by \":is()\""
        ),
        1,
        5,
        1,
        18
      )]
    );
  }

  #[test]
  fn rejects_tree_structural_pseudo_classes_on_pseudo_elements() {
    assert_eq!(
      lint("::before:first-child {}"),
      vec![(
        rejected(
          "::before:first-child",
          "",
          "\":first-child\" never matches pseudo-elements"
        ),
        1,
        1,
        1,
        21
      )]
    );
  }

  #[test]
  fn rejects_ancestors_of_the_shadow_host() {
    let reason = "the shadow host has no ancestors or siblings in its shadow tree";
    assert_eq!(
      lint("a :host {}"),
      vec![(rejected("a :host", "", reason), 1, 1, 1, 8)]
    );
    assert_eq!(
      lint("a { & :host {} }"),
      vec![(rejected("& :host", "a :host", reason), 1, 5, 1, 12)]
    );
    assert_eq!(
      lint("a { & /* foo */ :host {} }"),
      vec![(
        rejected("& /* foo */ :host", "a :host", reason),
        1,
        5,
        1,
        22
      )]
    );
  }

  #[test]
  fn rejects_features_compounded_with_the_shadow_host() {
    assert_eq!(
      lint("a:host {}"),
      vec![(
        rejected("a:host", "", "\"a\" never matches the shadow host"),
        1,
        1,
        1,
        7
      )]
    );
    assert_eq!(
      lint(":host.foo {}"),
      vec![(
        rejected(":host.foo", "", "\".foo\" never matches the shadow host"),
        1,
        1,
        1,
        10
      )]
    );
  }

  #[test]
  fn rejects_the_shadow_host_within_slotted() {
    assert_eq!(
      lint("::slotted(:host) {}"),
      vec![(
        rejected(
          "::slotted(:host)",
          "",
          "slotted elements are never the shadow host"
        ),
        1,
        1,
        1,
        17
      )]
    );
  }

  #[test]
  fn rejects_pseudo_classes_on_elements_they_never_match() {
    let reason = "\":enabled\" never matches \"label\" elements";
    assert_eq!(
      lint("label:enabled {}"),
      vec![(rejected("label:enabled", "", reason), 1, 1, 1, 14)]
    );
    assert_eq!(
      lint("label { &:enabled {} }"),
      vec![(rejected("&:enabled", "label:enabled", reason), 1, 9, 1, 18)]
    );
    assert_eq!(
      lint(":is(label):enabled {}"),
      vec![(rejected(":is(label):enabled", "", reason), 1, 1, 1, 19)]
    );
    assert_eq!(
      lint(":is(label, button):enabled {}"),
      vec![(
        rejected(":is(label, button):enabled", "", reason),
        1,
        1,
        1,
        27
      )]
    );
    assert_eq!(
      lint("label/* foo */:enabled {}"),
      vec![(rejected("label/* foo */:enabled", "", reason), 1, 1, 1, 23)]
    );
  }

  #[test]
  fn echoes_the_source_case_in_the_reason() {
    assert_eq!(
      lint("LABEL:ENABLED {}"),
      vec![(
        rejected(
          "LABEL:ENABLED",
          "",
          "\":ENABLED\" never matches \"LABEL\" elements"
        ),
        1,
        1,
        1,
        14
      )]
    );
  }

  #[test]
  fn rejects_pseudo_classes_that_never_match_the_same_element() {
    assert_eq!(
      lint(":any-link:checked {}"),
      vec![(
        rejected(
          ":any-link:checked",
          "",
          "\":any-link\" and \":checked\" never match the same element"
        ),
        1,
        1,
        1,
        18
      )]
    );
  }

  #[test]
  fn rejects_an_unmatchable_has_argument() {
    assert_eq!(
      lint("a:has(label:enabled) {}"),
      vec![(
        rejected(
          "a:has(label:enabled)",
          "",
          "\":enabled\" never matches \"label\" elements"
        ),
        1,
        1,
        1,
        21
      )]
    );
  }

  #[test]
  fn reports_each_selector_in_a_list() {
    assert_eq!(
      lint("label:enabled, p:checked {}"),
      vec![
        (
          rejected(
            "label:enabled",
            "",
            "\":enabled\" never matches \"label\" elements"
          ),
          1,
          1,
          1,
          14
        ),
        (
          rejected("p:checked", "", "\":checked\" never matches \"p\" elements"),
          1,
          16,
          1,
          25
        ),
      ]
    );
  }
}
