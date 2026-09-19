use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};
use crate::selector::{
  Pseudo, PseudoArg, Selector, SelectorList, SelectorNode, any_pseudo, is_standard_syntax_selector,
  parse_selector_list, walk_pseudos,
};
use crate::style_rules::scan_style_rules;

/// Disallow invalid selectors.
///
/// Equivalent to Stylelint's `selector-no-invalid` rule. Selectors defined in
/// the CSS specifications, up to and including Editor's Drafts, are valid.
pub struct SelectorNoInvalid;

/// Pseudo-selectors whose arguments are unforgiving, excluding the deprecated
/// `:host-context()`; see
/// https://drafts.csswg.org/selectors/#typedef-forgiving-selector-list
const UNFORGIVING_ARGUMENT_PSEUDOS: &[&str] = &[
  "has",
  "host",
  "not",
  "nth-child",
  "nth-last-child",
  "slotted",
];

/// Pseudo-selectors whose argument must be a compound selector; see
/// https://drafts.csswg.org/css-shadow-1/
const COMPOUND_ARGUMENT_PSEUDOS: &[&str] = &["host", "slotted"];

/// Pseudo-elements that are backed by real elements and so may be followed by
/// any other pseudo-element; see https://drafts.csswg.org/css-pseudo/#element-backed
const ELEMENT_BACKED_PSEUDO_ELEMENTS: &[&str] =
  &["details-content", "file-selector-button", "part"];

/// Pseudo-elements defined as sub-pseudo-elements of another pseudo-element,
/// e.g. `::before::marker`; see https://drafts.csswg.org/selectors/#sub-pseudo-elements
const SUB_PSEUDO_ELEMENTS: &[&str] = &["marker", "scroll-marker"];

/// The identifiers `:dir()` accepts.
const DIR_IDENTIFIERS: &[&str] = &["ltr", "rtl"];

impl Rule for SelectorNoInvalid {
  fn name(&self) -> &'static str {
    "selector-no-invalid"
  }

  fn description(&self) -> &'static str {
    "Disallow invalid selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Checks every style rule prelude, reporting selectors that fail to parse
  /// or that break a structural rule of the selector grammar.
  ///
  /// Preludes are read from the source text rather than the parsed AST: the
  /// CSS parser drops rules whose selectors it cannot parse, which are exactly
  /// the ones this rule exists to report.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for raw in scan_style_rules(ctx.source, ctx.syntax) {
      self.check_prelude(&raw.prelude, raw.offset, &mut diagnostics);
    }
    diagnostics
  }
}

impl SelectorNoInvalid {
  fn check_prelude(&self, selector: &str, base: usize, diagnostics: &mut Vec<Diagnostic>) {
    if !is_standard_syntax_selector(selector) {
      return;
    }

    let list = match parse_selector_list(selector) {
      Ok(list) => list,
      Err(err) => {
        let end = (err.offset + 1).min(selector.len());
        diagnostics.push(self.diagnostic(
          selector,
          &err.reason,
          Span::from_range(base + err.offset, base + end),
        ));
        return;
      }
    };

    for sel in &list.selectors {
      let source = &selector[sel.offset..sel.end];
      let span = Span::from_range(base + sel.offset, base + sel.end);
      let mut complain = |reason: String| {
        diagnostics.push(self.diagnostic(source, &reason, span));
      };
      check_unforgiving_arguments(sel, &mut complain);
      check_top_level_selector(sel, &mut complain);
      check_dir_pseudo_classes(sel, &mut complain);
    }
  }

  fn diagnostic(&self, selector: &str, reason: &str, span: Span) -> Diagnostic {
    Diagnostic::new(
      self.name(),
      format!("Invalid selector \"{selector}\", {reason}"),
    )
    .severity(self.default_severity())
    .span(span)
  }
}

/// Reports pseudo-selectors whose unforgiving arguments contain something the
/// grammar does not allow there.
fn check_unforgiving_arguments(selector: &Selector, complain: &mut dyn FnMut(String)) {
  walk_pseudos(&selector.nodes, &mut |visit| {
    if visit.pseudo.is_one_of(UNFORGIVING_ARGUMENT_PSEUDOS)
      && let Some(reason) = invalid_argument_reason(visit.pseudo)
    {
      complain(reason);
    }
    true
  });
}

/// The reason a pseudo-selector's argument is invalid, if any.
fn invalid_argument_reason(pseudo: &Pseudo) -> Option<String> {
  let list = pseudo.selectors()?;

  if list.any_node(SelectorNode::is_pseudo_element) {
    return Some(format!(
      "pseudo-elements are invalid within \"{}\"",
      pseudo.display_name()
    ));
  }

  if pseudo.is("has") && contains_has(list) {
    return Some("\":has()\" is invalid within \":has()\"".to_string());
  }

  if pseudo.is_one_of(COMPOUND_ARGUMENT_PSEUDOS) && has_argument_combinator(list) {
    return Some(format!(
      "combinators are invalid within \"{}\"",
      pseudo.display_name()
    ));
  }

  None
}

/// Whether a `:has()` argument contains another `:has()` at any depth; see
/// https://drafts.csswg.org/selectors/#relational
fn contains_has(list: &SelectorList) -> bool {
  list.selectors.iter().any(|selector| {
    any_pseudo(&selector.nodes, |pseudo| {
      !pseudo.element && pseudo.is("has")
    })
  })
}

/// Whether the first selector of an argument list contains a combinator.
fn has_argument_combinator(list: &SelectorList) -> bool {
  list
    .selectors
    .first()
    .is_some_and(|selector| selector.nodes.iter().any(SelectorNode::is_combinator))
}

/// Combinators and further pseudo-elements after a pseudo-element are
/// invalid; see https://drafts.csswg.org/selectors/#pseudo-element-structure
/// and https://drafts.csswg.org/selectors/#sub-pseudo-elements
fn check_top_level_selector(selector: &Selector, complain: &mut dyn FnMut(String)) {
  if selector.nodes.contains(&SelectorNode::Nesting) {
    return;
  }

  let mut last_pseudo_element: Option<&Pseudo> = None;
  let mut previous: Option<&SelectorNode> = None;

  for node in &selector.nodes {
    match node {
      SelectorNode::Comment(_) => continue,
      SelectorNode::Pseudo(pseudo) if pseudo.element => {
        if let Some(SelectorNode::Pseudo(prev)) = previous
          && prev.element
          && is_invalid_sub_pseudo_element(prev, pseudo)
        {
          complain(format!(
            "\"{}\" is invalid after \"{}\"",
            pseudo.display_name(),
            prev.display_name()
          ));
          return;
        }
        last_pseudo_element = Some(pseudo);
      }
      SelectorNode::Combinator(_) => {
        if let Some(last) = last_pseudo_element {
          complain(format!(
            "combinators are invalid after \"{}\"",
            last.display_name()
          ));
          return;
        }
      }
      _ => {}
    }
    previous = Some(node);
  }
}

/// Whether `second`, compounded to `first`, is not a defined sub-pseudo-element.
fn is_invalid_sub_pseudo_element(first: &Pseudo, second: &Pseudo) -> bool {
  !first.is_one_of(ELEMENT_BACKED_PSEUDO_ELEMENTS) && !second.is_one_of(SUB_PSEUDO_ELEMENTS)
}

/// Reports `:dir()` pseudo-classes whose argument is not `ltr` or `rtl`.
fn check_dir_pseudo_classes(selector: &Selector, complain: &mut dyn FnMut(String)) {
  walk_pseudos(&selector.nodes, &mut |visit| {
    let pseudo = visit.pseudo;
    if !pseudo.element && pseudo.is("dir") && !is_valid_dir_argument(&pseudo.arg) {
      let expected = DIR_IDENTIFIERS
        .iter()
        .map(|id| format!("\"{id}\""))
        .collect::<Vec<_>>()
        .join(" or ");
      complain(format!(
        "expected {expected} within \"{}\"",
        pseudo.display_name()
      ));
    }
    true
  });
}

fn is_valid_dir_argument(arg: &PseudoArg) -> bool {
  match arg {
    PseudoArg::Raw(raw) => DIR_IDENTIFIERS
      .iter()
      .any(|id| id.eq_ignore_ascii_case(raw.trim())),
    _ => false,
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
    let ctx = RuleContext {
      file_path: "t.css",
      source: css,
      syntax,
      options: Some(&serde_json::Value::Bool(true)),
    };
    let index = SourceLineIndex::build(css);
    SelectorNoInvalid
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

  fn rejected(selector: &str, reason: &str) -> String {
    format!("Invalid selector \"{selector}\", {reason}")
  }

  #[test]
  fn accepts_valid_selectors() {
    for css in [
      "a {}",
      ".foo {}",
      "a, b {}",
      "a > b {}",
      "a + b {}",
      "a ~ b {}",
      "[foo] {}",
      "[foo=\"bar\"] {}",
      ":hover {}",
      ":nth-child(2n+1) {}",
      ":not(.foo) {}",
      ":is(a, b) {}",
      ":has(> a) {}",
      "::before {}",
      "& a {}",
      "&:hover {}",
      ":dir(ltr) {}",
      ":dir(rtl) {}",
      ":dir(rtl), :lang(en) {}",
      "@keyframes foo { from {} 50% {} to {} }",
      ":nth-child(2n of .foo) {}",
      ":host {}",
      ":host(.foo) {}",
      ":host-context(::before) {}",
      "::slotted(.foo) {}",
      "::before::marker {}",
      "::part(foo)::before {}",
    ] {
      assert_eq!(lint(css), vec![], "should accept {css:?}");
    }
  }

  #[test]
  fn accepts_scss_specific_selectors() {
    for css in ["#{$foo} {}", "%foo {}"] {
      assert_eq!(
        lint_with(css, Syntax::Scss),
        vec![],
        "should accept {css:?}"
      );
    }
  }

  #[test]
  fn rejects_a_stray_closing_parenthesis() {
    assert_eq!(
      lint("a ) b {}"),
      vec![(rejected("a ) b", "unexpected input"), 1, 3, 1, 4)]
    );
  }

  #[test]
  fn rejects_a_leading_comma() {
    assert_eq!(
      lint(", a {}"),
      vec![(rejected(", a", "selector is expected"), 1, 1, 1, 2)]
    );
  }

  #[test]
  fn rejects_a_trailing_operator_in_anb() {
    assert_eq!(
      lint(":nth-child(2n+) {}"),
      vec![(
        rejected(":nth-child(2n+)", "integer is expected"),
        1,
        15,
        1,
        16
      )]
    );
  }

  #[test]
  fn rejects_an_attribute_name_starting_with_a_digit() {
    assert_eq!(
      lint("[0foo] {}"),
      vec![(rejected("[0foo]", "identifier is expected"), 1, 2, 1, 3)]
    );
  }

  #[test]
  fn rejects_a_doubled_attribute_operator() {
    assert_eq!(
      lint("[foo==bar] {}"),
      vec![(rejected("[foo==bar]", "identifier is expected"), 1, 6, 1, 7)]
    );
  }

  #[test]
  fn rejects_a_doubled_class_dot() {
    assert_eq!(
      lint(".foo..bar {}"),
      vec![(rejected(".foo..bar", "identifier is expected"), 1, 6, 1, 7)]
    );
  }

  #[test]
  fn rejects_an_empty_dir_argument() {
    assert_eq!(
      lint(":dir() {}"),
      vec![(
        rejected(":dir()", "expected \"ltr\" or \"rtl\" within \":dir()\""),
        1,
        1,
        1,
        7
      )]
    );
  }

  #[test]
  fn rejects_an_invalid_dir_argument() {
    assert_eq!(
      lint(":dir(foo) {}"),
      vec![(
        rejected(":dir(foo)", "expected \"ltr\" or \"rtl\" within \":dir()\""),
        1,
        1,
        1,
        10
      )]
    );
  }

  #[test]
  fn rejects_each_invalid_dir_in_a_list() {
    assert_eq!(
      lint(":dir(foo), :DIR(bar) {}"),
      vec![
        (
          rejected(":dir(foo)", "expected \"ltr\" or \"rtl\" within \":dir()\""),
          1,
          1,
          1,
          10
        ),
        (
          rejected(":DIR(bar)", "expected \"ltr\" or \"rtl\" within \":dir()\""),
          1,
          12,
          1,
          21
        ),
      ]
    );
  }

  #[test]
  fn rejects_pseudo_elements_within_not() {
    assert_eq!(
      lint(":not(::before) {}"),
      vec![(
        rejected(
          ":not(::before)",
          "pseudo-elements are invalid within \":not()\""
        ),
        1,
        1,
        1,
        15
      )]
    );
  }

  #[test]
  fn rejects_pseudo_elements_within_has() {
    assert_eq!(
      lint(".foo:has(::before) {}"),
      vec![(
        rejected(
          ".foo:has(::before)",
          "pseudo-elements are invalid within \":has()\""
        ),
        1,
        1,
        1,
        19
      )]
    );
  }

  #[test]
  fn rejects_pseudo_elements_within_nth_child_of() {
    assert_eq!(
      lint(":nth-child(2n of ::before) {}"),
      vec![(
        rejected(
          ":nth-child(2n of ::before)",
          "pseudo-elements are invalid within \":nth-child()\""
        ),
        1,
        1,
        1,
        27
      )]
    );
  }

  #[test]
  fn rejects_pseudo_elements_within_host() {
    assert_eq!(
      lint(":host(::before) {}"),
      vec![(
        rejected(
          ":host(::before)",
          "pseudo-elements are invalid within \":host()\""
        ),
        1,
        1,
        1,
        16
      )]
    );
  }

  #[test]
  fn rejects_pseudo_elements_within_slotted() {
    assert_eq!(
      lint("::slotted(::before) {}"),
      vec![(
        rejected(
          "::slotted(::before)",
          "pseudo-elements are invalid within \"::slotted()\""
        ),
        1,
        1,
        1,
        20
      )]
    );
  }

  #[test]
  fn rejects_has_within_has() {
    assert_eq!(
      lint(".foo:has(.bar:has(.baz)) {}"),
      vec![(
        rejected(
          ".foo:has(.bar:has(.baz))",
          "\":has()\" is invalid within \":has()\""
        ),
        1,
        1,
        1,
        25
      )]
    );
  }

  #[test]
  fn rejects_combinators_within_slotted() {
    assert_eq!(
      lint("::slotted(a .foo) {}"),
      vec![(
        rejected(
          "::slotted(a .foo)",
          "combinators are invalid within \"::slotted()\""
        ),
        1,
        1,
        1,
        18
      )]
    );
    assert_eq!(
      lint("::slotted(> a) {}"),
      vec![(
        rejected(
          "::slotted(> a)",
          "combinators are invalid within \"::slotted()\""
        ),
        1,
        1,
        1,
        15
      )]
    );
  }

  #[test]
  fn rejects_combinators_within_host() {
    assert_eq!(
      lint(":host(a .foo) {}"),
      vec![(
        rejected(
          ":host(a .foo)",
          "combinators are invalid within \":host()\""
        ),
        1,
        1,
        1,
        14
      )]
    );
  }

  #[test]
  fn rejects_combinators_after_a_pseudo_element() {
    assert_eq!(
      lint("a::after .foo {}"),
      vec![(
        rejected("a::after .foo", "combinators are invalid after \"::after\""),
        1,
        1,
        1,
        14
      )]
    );
    assert_eq!(
      lint("::slotted(a) .foo {}"),
      vec![(
        rejected(
          "::slotted(a) .foo",
          "combinators are invalid after \"::slotted()\""
        ),
        1,
        1,
        1,
        18
      )]
    );
  }

  #[test]
  fn rejects_an_undefined_sub_pseudo_element() {
    assert_eq!(
      lint("::after::before {}"),
      vec![(
        rejected(
          "::after::before",
          "\"::before\" is invalid after \"::after\""
        ),
        1,
        1,
        1,
        16
      )]
    );
  }

  #[test]
  fn rejects_within_a_forgiving_selector_list() {
    assert_eq!(
      lint(":is(:not(::before)) {}"),
      vec![(
        rejected(
          ":is(:not(::before))",
          "pseudo-elements are invalid within \":not()\""
        ),
        1,
        1,
        1,
        20
      )]
    );
    assert_eq!(
      lint(".foo:has(:where(.bar:has(.baz))) {}"),
      vec![(
        rejected(
          ".foo:has(:where(.bar:has(.baz)))",
          "\":has()\" is invalid within \":has()\""
        ),
        1,
        1,
        1,
        33
      )]
    );
  }

  #[test]
  fn lowercases_the_pseudo_name_in_the_reason() {
    assert_eq!(
      lint(":NOT(::BEFORE) {}"),
      vec![(
        rejected(
          ":NOT(::BEFORE)",
          "pseudo-elements are invalid within \":not()\""
        ),
        1,
        1,
        1,
        15
      )]
    );
  }

  #[test]
  fn reports_every_reason_for_one_selector() {
    assert_eq!(
      lint("::slotted(a .foo) .foo {}"),
      vec![
        (
          rejected(
            "::slotted(a .foo) .foo",
            "combinators are invalid within \"::slotted()\""
          ),
          1,
          1,
          1,
          23
        ),
        (
          rejected(
            "::slotted(a .foo) .foo",
            "combinators are invalid after \"::slotted()\""
          ),
          1,
          1,
          1,
          23
        ),
      ]
    );
  }
}
