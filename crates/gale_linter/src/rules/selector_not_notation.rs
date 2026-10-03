use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};
use crate::selector::postcss::{self, Kind, Node, Selector};
use crate::standard_syntax::is_standard_syntax_selector;

/// Specify simple or complex notation for `:not()` pseudo-class.
///
/// Equivalent to Stylelint's `selector-not-notation` rule.
///
/// - `"complex"` (default): prefer list arguments — flag chained
///   `:not(.a):not(.b)`; the fix merges the chain into `:not(.a, .b)`.
/// - `"simple"`: prefer chained notation — flag `:not()` whose argument is a
///   list or anything but one simple selector; the fix splits a list of
///   simple selectors into `:not(.a):not(.b)`.
pub struct SelectorNotNotation;

impl Rule for SelectorNotNotation {
  fn name(&self) -> &'static str {
    "selector-not-notation"
  }

  fn description(&self) -> &'static str {
    "Specify simple or complex notation for :not() pseudo-class"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags `:not()` written in the notation the option forbids, in every
  /// style rule's selector as written. Interpolated selectors are skipped.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let simple = ctx.primary_option_str() == Some("simple");
    let message = if simple {
      "Expected simple :not() pseudo-class notation"
    } else {
      "Expected complex :not() pseudo-class notation"
    };

    let mut diags = Vec::new();
    for rule in &ctx.scanned_rules().style_rules {
      if !rule.prelude.to_ascii_lowercase().contains(":not(")
        || !is_standard_syntax_selector(&rule.prelude)
      {
        continue;
      }
      let Some(selectors) = postcss::parse(&rule.prelude, rule.offset) else {
        continue;
      };
      postcss::walk(&selectors, &mut |visit| {
        let not = visit.node;
        if !is_not(not) {
          return;
        }
        let fix = if simple {
          let list = not.args.as_ref().map_or(&[][..], |a| &a.selectors[..]);
          if is_simple(list) {
            return;
          }
          simple_fix(ctx.source, visit.siblings, visit.index, list)
        } else {
          let Some(prev) = visit.prev().filter(|prev| is_not(prev)) else {
            return;
          };
          // The first report of a chain carries the fix for the whole chain.
          let chain_start = visit.index - 1;
          let starts_chain = chain_start == 0 || !is_not(&visit.siblings[chain_start - 1]);
          if starts_chain {
            complex_fix(ctx.source, prev, &visit.siblings[visit.index..])
          } else {
            None
          }
        };
        let mut diag = Diagnostic::new(self.name(), message)
          .severity(self.default_severity())
          .span(Span::from_range(not.start, not.end));
        if let Some(fix) = fix {
          diag = diag.fix(fix);
        }
        diags.push(diag);
      });
    }
    diags
  }
}

/// Whether a node is the `:not()` pseudo-class.
fn is_not(node: &Node) -> bool {
  is_pseudo_class(node) && node.value.eq_ignore_ascii_case(":not")
}

/// Whether a pseudo is a pseudo-class, as postcss-selector-parser tells:
/// not `::name`, nor one of the four legacy single-colon pseudo-elements.
fn is_pseudo_class(node: &Node) -> bool {
  node.kind == Kind::Pseudo
    && !node.value.starts_with("::")
    && ![":before", ":after", ":first-letter", ":first-line"]
      .iter()
      .any(|legacy| node.value.eq_ignore_ascii_case(legacy))
}

/// Stylelint's `isSimpleSelector`: a pseudo-class, attribute, class,
/// universal, id or type selector.
fn is_simple_selector(node: &Node) -> bool {
  is_pseudo_class(node)
    || matches!(
      node.kind,
      Kind::Attribute | Kind::Class | Kind::Universal | Kind::Id | Kind::Tag
    )
}

/// Stylelint's `isSimple`: an argument of at most one simple selector that
/// is not itself `:not()`.
fn is_simple(list: &[Selector]) -> bool {
  match list {
    [] => true,
    [only] => match only.nodes.as_slice() {
      [] => true,
      [first] => is_simple_selector(first) && !is_not(first),
      _ => false,
    },
    _ => false,
  }
}

/// A selector of a `:not()` argument as the fixes print it: from its first
/// node, without the whitespace before it, to its last, keeping whitespace
/// after the last node when there is more than one (postcss-selector-parser
/// only resets the first node's spacing).  `None` for an empty selector.
fn selector_text<'a>(source: &'a str, selector: &Selector) -> Option<&'a str> {
  let first = selector.nodes.first()?;
  let last = selector.nodes.last()?;
  let mut end = last.end;
  if selector.nodes.len() > 1 {
    let rest = source.get(end..)?;
    end += rest.len() - rest.trim_start().len();
  }
  source.get(first.start..end)
}

/// The fix for `"simple"`: keep the first simple selector in this `:not()`
/// and add a `:not()` for each other one after the chain of `:not()`s this
/// one starts.  Only a list whose selectors are all single nodes, or whose
/// second selector is empty, can be split.
fn simple_fix(source: &str, siblings: &[Node], index: usize, list: &[Selector]) -> Option<Fix> {
  let second = list.get(1)?;
  if !(second.nodes.is_empty() || list.iter().all(|s| s.nodes.len() == 1)) {
    return None;
  }
  let not = &siblings[index];
  let simple: Vec<&str> = list
    .iter()
    .filter(|s| s.nodes.first().is_some_and(is_simple_selector))
    .map(|s| selector_text(source, s))
    .collect::<Option<_>>()?;
  let (first, rest) = simple.split_first()?;
  let chain_end = siblings[index..]
    .iter()
    .take_while(|n| is_not(n))
    .last()
    .unwrap_or(not);
  let mut edits = vec![Edit::new(
    Span::from_range(not.start, not.end),
    format!("{}({first})", not.value),
  )];
  if !rest.is_empty() {
    // Each added `:not()` is a clone of the chain's last one, whitespace
    // after it (before a `,` or `)`) included.
    let after = source.get(chain_end.end..).unwrap_or("");
    let space = &after[..after.len() - after.trim_start().len()];
    let trailing = if after[space.len()..].starts_with([',', ')']) {
      space
    } else {
      ""
    };
    let added: String = rest
      .iter()
      .map(|s| format!("{}({s}){trailing}", chain_end.value))
      .collect();
    edits.push(Edit::new(
      Span::new(chain_end.end + trailing.len(), 0),
      added,
    ));
  }
  Some(Fix::new(
    "Split the :not() list into chained :not()s",
    edits,
  ))
}

/// The fix for `"complex"`: merge `first` and the chain of `:not()`s after
/// it (`chain`) into one `:not()` with a selector list.  None when `first`'s
/// argument is empty, as Stylelint leaves that alone.
fn complex_fix(source: &str, first: &Node, chain: &[Node]) -> Option<Fix> {
  let head = first.args.as_ref()?.selectors.first()?;
  if head.nodes.is_empty() {
    return None;
  }
  let mut items = Vec::new();
  let mut last = first;
  for not in std::iter::once(first).chain(chain.iter().take_while(|n| is_not(n))) {
    for selector in &not.args.as_ref()?.selectors {
      items.push(selector_text(source, selector)?);
    }
    last = not;
  }
  Some(Fix::new(
    "Merge the chained :not()s into one",
    vec![Edit::new(
      Span::from_range(first.start, last.end),
      format!("{}({})", first.value, items.join(", ")),
    )],
  ))
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;

  use crate::testing::{fix, lint};

  const RULE: &str = "selector-not-notation";

  #[test]
  fn simple_splits_lists_of_simple_selectors() {
    let simple = serde_json::json!("simple");
    assert_eq!(
      fix(RULE, simple.clone(), "p, img:not(a\n, div) {}", Syntax::Css),
      "p, img:not(a):not(div) {}"
    );
    assert_eq!(
      fix(
        RULE,
        simple.clone(),
        ":not(.bar, .baz) .qux :not(.foo) {}",
        Syntax::Css
      ),
      ":not(.bar):not(.baz) .qux :not(.foo) {}"
    );
    assert_eq!(
      fix(RULE, simple.clone(), ":not(a ,) {}", Syntax::Css),
      ":not(a) {}"
    );
    assert_eq!(
      fix(RULE, simple.clone(), ":not(a, b) , p {}", Syntax::Css),
      ":not(a) :not(b) , p {}"
    );
    let warnings = lint(RULE, simple.clone(), "p, :not(a, div) {}", Syntax::Css);
    assert_eq!(warnings.len(), 1);
    assert_eq!(
      warnings[0].message,
      "Expected simple :not() pseudo-class notation"
    );
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (3, 12));
  }

  #[test]
  fn simple_reports_but_cannot_fix_other_arguments() {
    let simple = serde_json::json!("simple");
    for css in [
      ":not(:not()) {}",
      ":not(::before) {}",
      ":not(:first-line) {}",
      ":not(a.foo) {}",
    ] {
      let warnings = lint(RULE, simple.clone(), css, Syntax::Css);
      assert_eq!(warnings.len(), 1, "{css}");
      assert!(warnings[0].fix.is_none(), "{css}");
    }
    for css in [
      ":not() {}",
      ":not( a ) {}",
      ":nOt(a) {}",
      ":not([title]) {}",
    ] {
      assert!(
        lint(RULE, simple.clone(), css, Syntax::Css).is_empty(),
        "{css}"
      );
    }
  }

  #[test]
  fn complex_merges_chains() {
    let complex = serde_json::json!("complex");
    assert_eq!(
      fix(
        RULE,
        complex.clone(),
        ":not( .foo ,:hover ):not(a,div) {}",
        Syntax::Css
      ),
      ":not(.foo, :hover, a, div) {}"
    );
    assert_eq!(
      fix(
        RULE,
        complex.clone(),
        "a:not(b):not(c):not(d) {}",
        Syntax::Css
      ),
      "a:not(b, c, d) {}"
    );
    for css in [
      ":not()::after {}",
      ":not(a, div) {}",
      ":not(a).foo:not(:empty) {}",
    ] {
      assert!(
        lint(RULE, complex.clone(), css, Syntax::Css).is_empty(),
        "{css}"
      );
    }
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!(["simple", { "disableFix": true }]);
    assert_eq!(
      lint(RULE, options.clone(), ":not(a, b) {}", Syntax::Css).len(),
      1
    );
    assert_eq!(
      fix(RULE, options, ":not(a, b) {}", Syntax::Css),
      ":not(a, b) {}"
    );
  }
}
