use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::data::is_known_html_element;
use crate::rule::{Rule, RuleContext};
use crate::selector::{
  Combinator, Selector, SelectorList, SelectorNode, is_standard_syntax_selector,
  parse_selector_list,
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

/// HTML elements Stylelint knows about that are not yet in the standard set.
const EXPERIMENTAL_HTML_ELEMENTS: &[&str] = &[
  "fencedframe",
  "geolocation",
  "install",
  "listbox",
  "model",
  "portal",
  "selectedcontent",
  "selectlist",
  "usermedia",
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

  /// Checks every style rule prelude, resolving nested selectors against
  /// their ancestors first.
  ///
  /// Preludes are read from the source text rather than the parsed AST so
  /// that selectors are seen exactly as written.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let rules = scan_style_rules(ctx.source, ctx.syntax);
    let mut diagnostics = Vec::new();
    for raw in &rules {
      self.check_rule(raw, &rules, &mut diagnostics);
    }
    diagnostics
  }
}

impl SelectorNoUnmatchable {
  fn check_rule(
    &self,
    raw: &RawStyleRule,
    rules: &[RawStyleRule],
    diagnostics: &mut Vec<Diagnostic>,
  ) {
    if !is_standard_syntax_selector(&raw.prelude) {
      return;
    }
    let Ok(own) = parse_selector_list(&raw.prelude) else {
      return;
    };
    let Some(parent) = resolve_ancestors(raw, rules) else {
      return;
    };

    for selector in &own.selectors {
      let (resolved, nested) = match &parent {
        Some(parent_list) => {
          let single = SelectorList {
            selectors: vec![selector.clone()],
          };
          (resolve_nested(&single, parent_list), true)
        }
        None => (
          SelectorList {
            selectors: vec![selector.clone()],
          },
          false,
        ),
      };

      let resolved_text = if nested {
        stringify_list(&resolved)
      } else {
        raw.prelude[selector.offset..selector.end].to_string()
      };
      if !resolved_text.contains(':') {
        continue;
      }

      let context = CheckContext {
        selector,
        resolved: &resolved,
        nested,
        resolved_text: &resolved_text,
        may_have_pseudo_element: resolved_text.contains("::"),
      };

      let reason = check_unrepresentable_pseudo_elements(&context)
        .or_else(|| check_shadow(&context))
        .or_else(|| check_pseudo_classing_pseudo_elements(&context))
        .or_else(|| check_applicable_elements(&context));

      if let Some(reason) = reason {
        let (text, start, end) = stripped_selector_source(selector, &raw.prelude);
        let subject = if nested {
          format!("\"{text}\" (\"{}\")", resolved_text.trim())
        } else {
          format!("\"{text}\"")
        };
        diagnostics.push(
          Diagnostic::new(
            self.name(),
            format!("Unmatchable selector {subject}, {reason}"),
          )
          .severity(self.default_severity())
          .span(Span::from_range(raw.offset + start, raw.offset + end)),
        );
      }
    }
  }
}

/// What the checks need to know about one selector of a rule.
struct CheckContext<'a> {
  /// The selector as written.
  selector: &'a Selector,
  /// The selector with nesting resolved.
  resolved: &'a SelectorList,
  /// Whether the rule is nested inside another style rule.
  nested: bool,
  /// `resolved` as text.
  resolved_text: &'a str,
  may_have_pseudo_element: bool,
}

/// The resolved selector list of a rule's enclosing style rules, outermost
/// first, or `None` if any ancestor cannot be resolved. `Some(None)` means
/// the rule is not nested.
fn resolve_ancestors(raw: &RawStyleRule, rules: &[RawStyleRule]) -> Option<Option<SelectorList>> {
  let mut chain = Vec::new();
  let mut parent = raw.parent;
  while let Some(index) = parent {
    chain.push(&rules[index]);
    parent = rules[index].parent;
  }
  chain.reverse();

  let mut resolved: Option<SelectorList> = None;
  for ancestor in chain {
    if !is_standard_syntax_selector(&ancestor.prelude) {
      return None;
    }
    let list = parse_selector_list(&ancestor.prelude).ok()?;
    resolved = Some(match resolved {
      Some(outer) => resolve_nested(&list, &outer),
      None => list,
    });
  }
  Some(resolved)
}

// ---------------------------------------------------------------------------
// Nesting resolution, following @csstools/selector-resolve-nested
// ---------------------------------------------------------------------------

/// Resolve `child` against `parent` per the CSS Nesting specification.
fn resolve_nested(child: &SelectorList, parent: &SelectorList) -> SelectorList {
  let mut selectors = Vec::with_capacity(child.selectors.len());
  for selector in &child.selectors {
    let mut nodes = selector.nodes.clone();
    if !contains_nesting(&nodes) {
      nodes.insert(
        0,
        SelectorNode::Combinator {
          kind: Combinator::Descendant,
          offset: 0,
          length: 0,
        },
      );
      nodes.insert(0, SelectorNode::Nesting { offset: 0 });
    } else if matches!(nodes.first(), Some(SelectorNode::Combinator { .. })) {
      nodes.insert(0, SelectorNode::Nesting { offset: 0 });
    }
    let nodes = replace_nesting(nodes, parent, false);
    selectors.push(Selector {
      nodes,
      offset: selector.offset,
      end: selector.end,
    });
  }
  SelectorList { selectors }
}

/// Whether a selector contains `&` at any depth.
fn contains_nesting(nodes: &[SelectorNode]) -> bool {
  nodes.iter().any(|node| match node {
    SelectorNode::Nesting { .. } => true,
    SelectorNode::PseudoClass {
      args: Some(list), ..
    }
    | SelectorNode::PseudoElement {
      args: Some(list), ..
    } => list.selectors.iter().any(|s| contains_nesting(&s.nodes)),
    _ => false,
  })
}

/// Replace every `&` with the parent selector, then sort the compound
/// selectors of any selector that changed.
fn replace_nesting(
  nodes: Vec<SelectorNode>,
  parent: &SelectorList,
  in_has: bool,
) -> Vec<SelectorNode> {
  let mut result = Vec::with_capacity(nodes.len());
  let mut replaced = false;
  for node in nodes {
    match node {
      SelectorNode::Nesting { .. } => {
        result.extend(prepare_parent_selectors(parent, in_has));
        replaced = true;
      }
      SelectorNode::PseudoClass {
        name,
        args: Some(list),
        raw_args,
        offset,
        length,
      } => {
        let is_has = name.eq_ignore_ascii_case("has");
        let list = replace_nesting_in_list(list, parent, is_has);
        result.push(SelectorNode::PseudoClass {
          name,
          args: Some(list),
          raw_args,
          offset,
          length,
        });
      }
      SelectorNode::PseudoElement {
        name,
        args: Some(list),
        raw_args,
        offset,
        length,
      } => {
        let list = replace_nesting_in_list(list, parent, false);
        result.push(SelectorNode::PseudoElement {
          name,
          args: Some(list),
          raw_args,
          offset,
          length,
        });
      }
      other => result.push(other),
    }
  }
  if replaced {
    result = sort_compound_selectors(result);
  }
  result
}

fn replace_nesting_in_list(
  list: SelectorList,
  parent: &SelectorList,
  in_has: bool,
) -> SelectorList {
  SelectorList {
    selectors: list
      .selectors
      .into_iter()
      .map(|s| Selector {
        nodes: replace_nesting(s.nodes, parent, in_has),
        offset: s.offset,
        end: s.end,
      })
      .collect(),
  }
}

/// The nodes that stand in for `&`: the parent inline when it is a single
/// compound selector, otherwise wrapped in `:is()`.
fn prepare_parent_selectors(parent: &SelectorList, force_is: bool) -> Vec<SelectorNode> {
  if force_is || !is_compound_selector_list(parent) {
    return vec![SelectorNode::PseudoClass {
      name: "is".to_string(),
      args: Some(parent.clone()),
      raw_args: Some(stringify_list(parent)),
      offset: 0,
      length: 0,
    }];
  }
  parent.selectors[0].nodes.clone()
}

/// Whether a selector list is exactly one compound selector.
fn is_compound_selector_list(list: &SelectorList) -> bool {
  if list.selectors.len() != 1 {
    return false;
  }
  !list.selectors[0].nodes.iter().any(|node| {
    matches!(
      node,
      SelectorNode::Combinator { .. } | SelectorNode::PseudoElement { .. }
    )
  })
}

/// Reorder each compound selector into canonical order after substitution,
/// merging duplicate universal and type selectors.
fn sort_compound_selectors(nodes: Vec<SelectorNode>) -> Vec<SelectorNode> {
  let mut groups: Vec<Vec<SelectorNode>> = Vec::new();
  let mut current: Vec<SelectorNode> = Vec::new();

  for node in nodes {
    match &node {
      SelectorNode::Combinator { .. } => {
        groups.push(std::mem::take(&mut current));
        groups.push(vec![node]);
      }
      SelectorNode::PseudoElement { .. } => {
        groups.push(std::mem::take(&mut current));
        current.push(node);
      }
      SelectorNode::Universal { .. }
        if current
          .iter()
          .any(|n| matches!(n, SelectorNode::Universal { .. })) => {}
      SelectorNode::Tag { .. }
        if current
          .iter()
          .any(|n| matches!(n, SelectorNode::Tag { .. })) =>
      {
        let list = SelectorList {
          selectors: vec![Selector {
            nodes: vec![node.clone()],
            offset: 0,
            end: 0,
          }],
        };
        current.push(SelectorNode::PseudoClass {
          name: "is".to_string(),
          raw_args: Some(stringify_list(&list)),
          args: Some(list),
          offset: 0,
          length: 0,
        });
      }
      _ => current.push(node),
    }
  }
  groups.push(current);

  let mut sorted = Vec::new();
  for mut group in groups {
    group.sort_by_key(selector_type_order);
    sorted.extend(group);
  }
  sorted
}

fn selector_type_order(node: &SelectorNode) -> u8 {
  match node {
    SelectorNode::Universal { .. } => 0,
    SelectorNode::Tag { .. } => 1,
    SelectorNode::PseudoElement { .. } => 2,
    SelectorNode::Nesting { .. } => 3,
    SelectorNode::Id { .. } => 4,
    SelectorNode::Class { .. } => 5,
    SelectorNode::Attribute { .. } => 6,
    SelectorNode::PseudoClass { .. } => 7,
    SelectorNode::Comment { .. } => 8,
    SelectorNode::Combinator { .. } => 9,
  }
}

// ---------------------------------------------------------------------------
// Serialisation
// ---------------------------------------------------------------------------

/// A selector list as text, with normalised spacing and comments dropped.
fn stringify_list(list: &SelectorList) -> String {
  list
    .selectors
    .iter()
    .map(|s| stringify_nodes(&s.nodes))
    .collect::<Vec<_>>()
    .join(",")
}

fn stringify_nodes(nodes: &[SelectorNode]) -> String {
  let mut out = String::new();
  for node in nodes {
    stringify_node(node, &mut out);
  }
  out
}

fn stringify_node(node: &SelectorNode, out: &mut String) {
  match node {
    SelectorNode::Tag { name, .. } => out.push_str(name),
    SelectorNode::Universal { .. } => out.push('*'),
    SelectorNode::Class { name, .. } => {
      out.push('.');
      out.push_str(name);
    }
    SelectorNode::Id { name, .. } => {
      out.push('#');
      out.push_str(name);
    }
    SelectorNode::Attribute { raw, .. } => out.push_str(raw),
    SelectorNode::PseudoClass {
      name,
      args,
      raw_args,
      ..
    } => {
      out.push(':');
      out.push_str(name);
      stringify_args(args, raw_args, out);
    }
    SelectorNode::PseudoElement {
      name,
      args,
      raw_args,
      ..
    } => {
      out.push_str("::");
      out.push_str(name);
      stringify_args(args, raw_args, out);
    }
    SelectorNode::Nesting { .. } => out.push('&'),
    SelectorNode::Combinator { kind, .. } => out.push_str(match kind {
      Combinator::Descendant => " ",
      Combinator::Child => " > ",
      Combinator::NextSibling => " + ",
      Combinator::SubsequentSibling => " ~ ",
      Combinator::Column => " || ",
    }),
    SelectorNode::Comment { .. } => {}
  }
}

fn stringify_args(args: &Option<SelectorList>, raw_args: &Option<String>, out: &mut String) {
  match (args, raw_args) {
    (Some(list), Some(raw)) if is_anb_argument(raw) => {
      out.push('(');
      out.push_str(raw);
      out.push(')');
      let _ = list;
    }
    (Some(list), _) => {
      out.push('(');
      out.push_str(&stringify_list(list));
      out.push(')');
    }
    (None, Some(raw)) => {
      out.push('(');
      out.push_str(raw);
      out.push(')');
    }
    (None, None) => {}
  }
}

/// Whether a raw pseudo argument is An+B notation with an `of` clause, which
/// is serialised as written rather than rebuilt from its parsed selectors.
fn is_anb_argument(raw: &str) -> bool {
  let lower = raw.to_ascii_lowercase();
  lower.contains(" of ") || lower.starts_with("of ")
}

/// The selector text Stylelint reports: from the first to the last
/// non-comment node, trimmed. Returns the text and its offsets within the
/// prelude.
fn stripped_selector_source(selector: &Selector, prelude: &str) -> (String, usize, usize) {
  let first = selector
    .nodes
    .iter()
    .find(|n| !matches!(n, SelectorNode::Comment { .. }));
  let last = selector
    .nodes
    .iter()
    .rev()
    .find(|n| !matches!(n, SelectorNode::Comment { .. }));
  let (start, end) = match (first, last) {
    (Some(first), Some(last)) => (first.offset(), node_end(last)),
    _ => (selector.offset, selector.end),
  };
  let text = prelude[start..end].trim().to_string();
  let end = start + text.len();
  (text, start, end)
}

/// The exclusive end offset of a node within its prelude.
fn node_end(node: &SelectorNode) -> usize {
  match node {
    SelectorNode::Tag { name, offset } => offset + name.len(),
    SelectorNode::Universal { offset } => offset + 1,
    SelectorNode::Class { name, offset } => offset + 1 + name.len(),
    SelectorNode::Id { name, offset } => offset + 1 + name.len(),
    SelectorNode::Attribute { raw, offset } => offset + raw.len(),
    SelectorNode::PseudoClass { offset, length, .. }
    | SelectorNode::PseudoElement { offset, length, .. } => offset + length,
    SelectorNode::Nesting { offset } => offset + 1,
    SelectorNode::Combinator { offset, length, .. } => offset + length,
    SelectorNode::Comment { raw, offset } => offset + raw.len(),
  }
}

// ---------------------------------------------------------------------------
// Walking
// ---------------------------------------------------------------------------

/// A pseudo node together with where it sits: the selector holding it and the
/// pseudo-classes enclosing it, nearest first.
struct PseudoVisit<'a> {
  node: &'a SelectorNode,
  siblings: &'a [SelectorNode],
  index: usize,
  ancestors: Vec<&'a SelectorNode>,
}

/// Visit every pseudo-class and pseudo-element in a selector list, in source
/// order, stopping once `visit` returns `false`.
fn walk_pseudos<'a>(
  list: &'a SelectorList,
  visit: &mut dyn FnMut(&PseudoVisit<'a>) -> bool,
) -> bool {
  for selector in &list.selectors {
    if !walk_selector_pseudos(&selector.nodes, Vec::new(), visit) {
      return false;
    }
  }
  true
}

fn walk_selector_pseudos<'a>(
  nodes: &'a [SelectorNode],
  ancestors: Vec<&'a SelectorNode>,
  visit: &mut dyn FnMut(&PseudoVisit<'a>) -> bool,
) -> bool {
  for (index, node) in nodes.iter().enumerate() {
    let args = match node {
      SelectorNode::PseudoClass { args, .. } | SelectorNode::PseudoElement { args, .. } => args,
      _ => continue,
    };
    let keep_going = visit(&PseudoVisit {
      node,
      siblings: nodes,
      index,
      ancestors: ancestors.clone(),
    });
    if !keep_going {
      return false;
    }
    if let Some(list) = args {
      // Only pseudo-classes count as ancestors; a pseudo-element breaks the chain.
      let inner_ancestors = if matches!(node, SelectorNode::PseudoClass { .. }) {
        let mut a = vec![node];
        a.extend(ancestors.iter().copied());
        a
      } else {
        Vec::new()
      };
      for selector in &list.selectors {
        if !walk_selector_pseudos(&selector.nodes, inner_ancestors.clone(), visit) {
          return false;
        }
      }
    }
  }
  true
}

/// The lower-cased name of a pseudo node without its colons.
fn pseudo_name(node: &SelectorNode) -> Option<String> {
  match node {
    SelectorNode::PseudoClass { name, .. } | SelectorNode::PseudoElement { name, .. } => {
      Some(name.to_ascii_lowercase())
    }
    _ => None,
  }
}

/// `:name` or `::name` as written, which Stylelint calls the node's value.
fn pseudo_value(node: &SelectorNode) -> String {
  match node {
    SelectorNode::PseudoClass { name, .. } => format!(":{name}"),
    SelectorNode::PseudoElement { name, .. } => format!("::{name}"),
    _ => String::new(),
  }
}

fn is_negation_or_relational(node: &SelectorNode) -> bool {
  pseudo_name(node)
    .map(|n| NEGATION_AND_RELATIONAL_PSEUDO_CLASSES.contains(&n.as_str()))
    .unwrap_or(false)
}

fn pseudo_args(node: &SelectorNode) -> Option<&SelectorList> {
  match node {
    SelectorNode::PseudoClass { args, .. } | SelectorNode::PseudoElement { args, .. } => {
      args.as_ref()
    }
    _ => None,
  }
}

// ---------------------------------------------------------------------------
// Compound selectors, following Stylelint's groupByCompoundSelectors with
// `groupNegationArguments: false`
// ---------------------------------------------------------------------------

/// Every compound selector reachable from a resolved selector list, with
/// `:has()` arguments and forgiving selector lists expanded.
fn compound_selectors(list: &SelectorList) -> Vec<Vec<&SelectorNode>> {
  let mut compounds = Vec::new();
  for selector in &list.selectors {
    let (terminated, current) = group_compounds(&selector.nodes);
    compounds.extend(terminated);
    compounds.extend(current);
  }
  compounds
    .into_iter()
    .map(|c| {
      c.into_iter()
        .filter(|n| !matches!(n, SelectorNode::Comment { .. }))
        .collect::<Vec<_>>()
    })
    .filter(|c| !c.is_empty())
    .collect()
}

type Compounds<'a> = Vec<Vec<&'a SelectorNode>>;

fn group_compounds(nodes: &[SelectorNode]) -> (Compounds<'_>, Compounds<'_>) {
  let mut terminated: Compounds = Vec::new();
  let mut current: Compounds = vec![Vec::new()];

  for node in nodes {
    if matches!(node, SelectorNode::Combinator { .. }) {
      terminated.append(&mut current);
      current = vec![Vec::new()];
      continue;
    }

    if matches!(node, SelectorNode::PseudoElement { .. }) {
      terminated.append(&mut current);
      current = vec![Vec::new()];
    }

    if let SelectorNode::PseudoClass {
      name,
      args: Some(list),
      ..
    } = node
    {
      if name.eq_ignore_ascii_case("has") && !list.selectors.is_empty() {
        for child in &list.selectors {
          let (mut child_terminated, mut child_current) = group_compounds(&child.nodes);
          terminated.append(&mut child_terminated);
          terminated.append(&mut child_current);
        }
        continue;
      }

      if !name.eq_ignore_ascii_case("not") {
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

// ---------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------

/// Pseudo-elements cannot be represented by `:is()`, `:where()` or `&`; see
/// https://drafts.csswg.org/selectors/#matches
fn check_unrepresentable_pseudo_elements(ctx: &CheckContext) -> Option<String> {
  if !ctx.may_have_pseudo_element {
    return None;
  }

  let own = SelectorList {
    selectors: vec![ctx.selector.clone()],
  };
  let mut reason = None;
  walk_pseudos(&own, &mut |visit| {
    if !matches!(visit.node, SelectorNode::PseudoElement { .. }) {
      return true;
    }
    if visit.ancestors.iter().any(|a| is_negation_or_relational(a)) {
      return true;
    }
    let Some(nearest) = visit.ancestors.first() else {
      return true;
    };
    let name = pseudo_name(nearest).unwrap_or_default();
    if FORGIVING_PSEUDO_CLASSES.contains(&name.as_str()) {
      reason = Some(format!(
        "pseudo-elements cannot be represented by \":{name}()\""
      ));
      return false;
    }
    true
  });

  if reason.is_some() || !ctx.nested {
    return reason;
  }

  find_unrepresentable_nesting_selector(ctx.resolved)
}

/// The nesting selector cannot represent pseudo-elements; see
/// https://drafts.csswg.org/css-nesting/#nest-selector
fn find_unrepresentable_nesting_selector(resolved: &SelectorList) -> Option<String> {
  let mut reason = None;
  walk_pseudos(resolved, &mut |visit| {
    if !matches!(visit.node, SelectorNode::PseudoClass { .. }) {
      return true;
    }
    let name = pseudo_name(visit.node).unwrap_or_default();
    if !FORGIVING_PSEUDO_CLASSES.contains(&name.as_str()) {
      return true;
    }
    if visit.ancestors.iter().any(|a| is_negation_or_relational(a)) {
      return true;
    }
    let contains_pseudo_element = pseudo_args(visit.node)
      .map(|list| {
        list.selectors.iter().any(|s| {
          s.nodes
            .iter()
            .any(|n| matches!(n, SelectorNode::PseudoElement { .. }))
        })
      })
      .unwrap_or(false);
    if !contains_pseudo_element {
      return true;
    }
    reason = Some("\"&\" cannot represent pseudo-elements".to_string());
    false
  });
  reason
}

/// The shadow host is featureless and has no selectable ancestors or
/// siblings; see https://drafts.csswg.org/css-shadow-1/#host-selector
fn check_shadow(ctx: &CheckContext) -> Option<String> {
  if !ctx.resolved_text.to_ascii_lowercase().contains(":host") {
    return None;
  }

  let mut reason = None;
  walk_pseudos(ctx.resolved, &mut |visit| {
    let name = pseudo_name(visit.node).unwrap_or_default();

    if name == "slotted" && matches!(visit.node, SelectorNode::PseudoElement { .. }) {
      if contains_host_pseudo_class(visit.node) {
        reason = Some("slotted elements are never the shadow host".to_string());
      }
      return false;
    }

    if name != "host" || !matches!(visit.node, SelectorNode::PseudoClass { .. }) {
      return true;
    }

    if visit.ancestors.iter().any(|a| is_negation_or_relational(a)) {
      return true;
    }

    if visit.siblings[..visit.index]
      .iter()
      .any(|n| matches!(n, SelectorNode::Combinator { .. }))
    {
      reason = Some("the shadow host has no ancestors or siblings in its shadow tree".to_string());
      return false;
    }

    if let Some(feature) = find_compound_feature_sibling(visit.siblings, visit.index) {
      reason = Some(format!(
        "\"{}\" never matches the shadow host",
        stringify_nodes(std::slice::from_ref(feature)).trim()
      ));
      return false;
    }

    true
  });
  reason
}

/// Whether a `::slotted()` argument contains `:host` at its top level.
fn contains_host_pseudo_class(slotted: &SelectorNode) -> bool {
  pseudo_args(slotted)
    .map(|list| {
      list.selectors.iter().any(|s| {
        s.nodes.iter().any(|n| {
          matches!(n, SelectorNode::PseudoClass { .. }) && pseudo_name(n).as_deref() == Some("host")
        })
      })
    })
    .unwrap_or(false)
}

/// A type, class, id or attribute selector compounded with the node at
/// `index`, if any.
fn find_compound_feature_sibling(siblings: &[SelectorNode], index: usize) -> Option<&SelectorNode> {
  for (i, sibling) in siblings[..index].iter().enumerate().rev() {
    if matches!(sibling, SelectorNode::Combinator { .. }) {
      break;
    }
    if is_feature_selector(siblings, i) {
      return Some(sibling);
    }
  }
  for (i, sibling) in siblings.iter().enumerate().skip(index + 1) {
    if matches!(
      sibling,
      SelectorNode::Combinator { .. } | SelectorNode::PseudoElement { .. }
    ) {
      break;
    }
    if is_feature_selector(siblings, i) {
      return Some(sibling);
    }
  }
  None
}

fn is_feature_selector(siblings: &[SelectorNode], index: usize) -> bool {
  match &siblings[index] {
    SelectorNode::Tag { .. } => is_standard_syntax_type_selector(siblings, index),
    SelectorNode::Class { .. } | SelectorNode::Id { .. } | SelectorNode::Attribute { .. } => true,
    _ => false,
  }
}

/// Whether a type selector is standard CSS rather than preprocessor syntax.
fn is_standard_syntax_type_selector(siblings: &[SelectorNode], index: usize) -> bool {
  let SelectorNode::Tag { name, .. } = &siblings[index] else {
    return false;
  };
  // `&-bar` is a nesting selector combined with a suffix.
  if index > 0 && matches!(siblings[index - 1], SelectorNode::Nesting { .. }) {
    return false;
  }
  if name.starts_with('%') {
    return false;
  }
  // Reference combinators like `/deep/`.
  if name.starts_with('/') && name.ends_with('/') {
    return false;
  }
  true
}

/// Tree-structural pseudo-classes never match pseudo-elements; see
/// https://drafts.csswg.org/selectors/#structural-pseudos
fn check_pseudo_classing_pseudo_elements(ctx: &CheckContext) -> Option<String> {
  if !ctx.may_have_pseudo_element {
    return None;
  }

  for compound in compound_selectors(ctx.resolved) {
    let Some(first) = compound.first() else {
      continue;
    };
    if !matches!(first, SelectorNode::PseudoElement { .. }) {
      continue;
    }
    let first_name = pseudo_name(first).unwrap_or_default();
    if ELEMENT_REPRESENTING_PSEUDO_ELEMENTS.contains(&first_name.as_str()) {
      continue;
    }
    let structural = compound.iter().find(|n| {
      matches!(n, SelectorNode::PseudoClass { .. })
        && pseudo_name(n)
          .map(|name| TREE_STRUCTURAL_PSEUDO_CLASSES.contains(&name.as_str()))
          .unwrap_or(false)
    });
    if let Some(node) = structural {
      return Some(format!(
        "\"{}\" never matches pseudo-elements",
        pseudo_value(node)
      ));
    }
  }
  None
}

/// Some pseudo-classes only match specific elements; see
/// https://html.spec.whatwg.org/multipage/semantics-other.html#pseudo-classes
fn check_applicable_elements(ctx: &CheckContext) -> Option<String> {
  for compound in compound_selectors(ctx.resolved) {
    if matches!(compound.first(), Some(SelectorNode::PseudoElement { .. })) {
      continue;
    }

    let applicable: Vec<(&SelectorNode, &'static [&'static str])> = compound
      .iter()
      .filter_map(|node| {
        if !matches!(node, SelectorNode::PseudoClass { .. }) {
          return None;
        }
        let name = pseudo_name(node)?;
        applicable_elements(&name).map(|elements| (*node, elements))
      })
      .collect();

    if applicable.is_empty() {
      continue;
    }

    let tag_index = compound
      .iter()
      .position(|n| matches!(n, SelectorNode::Tag { .. }));
    if let Some(index) = tag_index {
      let tag = compound[index];
      if let Some(tag_name) = html_type_selector_name(&compound, index) {
        for (node, elements) in &applicable {
          if !elements.contains(&tag_name.as_str()) {
            let SelectorNode::Tag { name, .. } = tag else {
              unreachable!()
            };
            return Some(format!(
              "\"{}\" never matches \"{name}\" elements",
              pseudo_value(node)
            ));
          }
        }
      }
    }

    for (i, (first_node, first_elements)) in applicable.iter().enumerate() {
      for (second_node, second_elements) in &applicable[i + 1..] {
        let disjoint = first_elements.iter().all(|e| !second_elements.contains(e));
        if disjoint {
          return Some(format!(
            "\"{}\" and \"{}\" never match the same element",
            pseudo_value(first_node),
            pseudo_value(second_node)
          ));
        }
      }
    }
  }
  None
}

/// The lower-cased name of a type selector when it is an HTML element.
fn html_type_selector_name(compound: &[&SelectorNode], index: usize) -> Option<String> {
  let SelectorNode::Tag { name, .. } = compound[index] else {
    return None;
  };
  if name.contains('|') {
    return None;
  }
  let owned: Vec<SelectorNode> = compound.iter().map(|n| (*n).clone()).collect();
  if !is_standard_syntax_type_selector(&owned, index) {
    return None;
  }
  let lower = name.to_ascii_lowercase();
  if is_known_html_element(&lower) || EXPERIMENTAL_HTML_ELEMENTS.contains(&lower.as_str()) {
    Some(lower)
  } else {
    None
  }
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
