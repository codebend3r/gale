use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{Accept, InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::selector::nesting::{resolve_nested, resolve_nested_list};
use crate::selector::{
  Selector, SelectorList, SelectorNode, is_standard_syntax_selector, parse_selector_list,
};
use crate::style_rules::RawStyleRule;
use crate::stylelint_version::stylelint_major_version;

const NAME: &str = "selector-max-attribute";

declare_rule! {
  /// Limit the number of attribute selectors in a selector.
  ///
  /// Equivalent to Stylelint's `selector-max-attribute` rule, whose counting
  /// changed in Stylelint 17:
  ///
  /// - Stylelint 16 resolves nested selectors and counts the attribute
  ///   selectors of each complex selector on its own: those at the top level,
  ///   and separately those of each selector inside `:not()`, `:is()`,
  ///   `:where()`, `:has()`, `:matches()` and the `of S` clause of
  ///   `:nth-child()` / `:nth-last-child()`.
  /// - Stylelint 17 counts every attribute selector in a selector as written,
  ///   at any depth.
  ///
  /// `ignoreAttributes` (names or `/regex/` entries) leaves attributes out of
  /// the count.
  pub struct SelectorMaxAttribute => SelectorMaxAttributeCheck {
    name: NAME,
    description: "Limit the number of attribute selectors in a selector",
    default_severity: Warning,
  }
}

/// `selector-max-attribute` as configured.
pub struct SelectorMaxAttributeCheck {
  /// The most attribute selectors allowed; with none configured the rule
  /// reports nothing.
  max: Option<u64>,
  /// The `ignoreAttributes` option.
  ignore: Matchers,
}

impl SelectorMaxAttributeCheck {
  /// Stylelint's options: a whole number of zero or more (tested by a
  /// function, so a problem reads `Invalid option`), then
  /// `ignoreAttributes`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max = options.primary_test(|value| Accept::NonNegativeInteger.accepts(value));
    let ignore = options.matchers("ignoreAttributes");
    options.finish()?;
    Ok(Self {
      max: max.and_then(serde_json::Value::as_u64),
      ignore,
    })
  }
}

/// Pseudo-classes whose selectors Stylelint 16 checks on their own
/// (`isContextFunctionalPseudoClass`).
const CONTEXT_FUNCTIONAL_PSEUDO_CLASSES: &[&str] = &[
  "has",
  "is",
  "matches",
  "not",
  "where",
  "nth-child",
  "nth-last-child",
];

/// The name an attribute selector tests: `type` for `[type="date"]`,
/// `href` for `[xlink|href]`.
fn attribute_name(raw: &str) -> &str {
  let inner = raw
    .strip_prefix('[')
    .unwrap_or(raw)
    .trim_end_matches(']')
    .trim_start();
  let end = inner
    .find(|c: char| "=~|^$*]".contains(c) || c.is_whitespace())
    .unwrap_or(inner.len());
  // `ns|name`: the name follows a lone `|` (not `|=`).
  let rest = &inner[end..];
  if let Some(after) = rest.strip_prefix('|')
    && !after.starts_with('=')
  {
    let name_end = after
      .find(|c: char| "=~|^$*]".contains(c) || c.is_whitespace())
      .unwrap_or(after.len());
    return &after[..name_end];
  }
  &inner[..end]
}

/// The options as the counting reads them.
struct Options<'a> {
  /// The most attribute selectors allowed.
  max: u64,
  /// The `ignoreAttributes` option.
  ignore: &'a Matchers,
}

impl Options<'_> {
  /// Whether `node` is an attribute selector that counts.
  fn counts(&self, node: &SelectorNode) -> bool {
    matches!(node, SelectorNode::Attribute(raw)
      if !self.ignore.matches(attribute_name(raw)))
  }

  /// Stylelint 17: every counted attribute selector in `nodes`, at any depth.
  fn count_deep(&self, nodes: &[SelectorNode]) -> u64 {
    nodes
      .iter()
      .map(|node| match node {
        SelectorNode::Pseudo(pseudo) => pseudo.selectors().map_or(0, |list| {
          list
            .selectors
            .iter()
            .map(|inner| self.count_deep(&inner.nodes))
            .sum()
        }),
        other => u64::from(self.counts(other)),
      })
      .sum()
  }

  /// Stylelint 16: the counts of `selector`'s own attribute selectors and,
  /// separately, of each selector inside a context-functional pseudo-class,
  /// at any depth.  Calls `found` with each count over the maximum.
  fn counts_by_context(&self, selector: &Selector, found: &mut impl FnMut(u64)) {
    let own = selector
      .nodes
      .iter()
      .filter(|node| self.counts(node))
      .count() as u64;
    if own > self.max {
      found(own);
    }
    self.inner_contexts(&selector.nodes, found);
  }

  /// [`Self::counts_by_context`] for the selectors inside the pseudo-classes
  /// among `nodes`.
  fn inner_contexts(&self, nodes: &[SelectorNode], found: &mut impl FnMut(u64)) {
    for node in nodes {
      let SelectorNode::Pseudo(pseudo) = node else {
        continue;
      };
      let Some(list) = pseudo.selectors() else {
        continue;
      };
      let context = !pseudo.element && pseudo.is_one_of(CONTEXT_FUNCTIONAL_PSEUDO_CLASSES);
      for inner in &list.selectors {
        if context {
          self.counts_by_context(inner, found);
        } else {
          self.inner_contexts(&inner.nodes, found);
        }
      }
    }
  }
}

impl SelectorMaxAttributeCheck {
  /// The report for `selector` of `raw`, written as `text`.
  fn report(&self, raw: &RawStyleRule, selector: &Selector, max: u64, v17: bool) -> Diagnostic {
    let text = &raw.prelude[selector.offset..selector.end];
    let message = if v17 {
      format!("Too many attribute selectors in \"{text}\", maximum {max}")
    } else {
      let noun = if max == 1 { "selector" } else { "selectors" };
      format!("Expected \"{text}\" to have no more than {max} attribute {noun}")
    };
    Diagnostic::new(NAME, message).span(Span::from_range(
      raw.offset + selector.offset,
      raw.offset + selector.end,
    ))
  }
}

impl Check for SelectorMaxAttributeCheck {
  /// Flags selectors holding more attribute selectors than allowed, counted
  /// the way the installed Stylelint counts them.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let Some(max) = self.max else {
      return vec![];
    };
    let options = Options {
      max,
      ignore: &self.ignore,
    };
    let v17 = stylelint_major_version() >= 17;

    let scanned = ctx.scanned_rules();
    let rules = &scanned.style_rules;
    let mut diags = Vec::new();
    // Each rule's selectors resolved against its ancestors, for its nested
    // rules to build on.  Parents precede their children in `rules`.
    let mut resolved: Vec<Option<SelectorList>> = Vec::with_capacity(rules.len());
    for raw in rules {
      let own = is_standard_syntax_selector(&raw.prelude)
        .then(|| parse_selector_list(&raw.prelude).ok())
        .flatten();
      let parent = raw.parent.and_then(|index| resolved[index].as_ref());
      let Some(own) = own else {
        resolved.push(None);
        continue;
      };
      if v17 {
        for selector in &own.selectors {
          if options.count_deep(&selector.nodes) > max {
            diags.push(self.report(raw, selector, max, true));
          }
        }
      } else if raw.parent.is_none() || parent.is_some() {
        for selector in &own.selectors {
          let nested = parent.map(|parent| resolve_nested(selector, parent));
          options.counts_by_context(nested.as_ref().unwrap_or(selector), &mut |_| {
            diags.push(self.report(raw, selector, max, false));
          });
        }
      }
      let entry = match (raw.parent, parent) {
        (None, _) => Some(own),
        (Some(_), Some(parent)) => Some(resolve_nested_list(&own, parent)),
        // An ancestor that cannot be resolved leaves this rule unresolved.
        (Some(_), None) => None,
      };
      resolved.push(entry);
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::style_rules::scan_style_rules;
  use gale_css_parser::Syntax;

  use crate::testing::invalid_options;

  /// The messages for `source` with `options`, counted the Stylelint 17
  /// way when `v17` is set and the Stylelint 16 way otherwise.
  fn lint(source: &str, options: serde_json::Value, v17: bool) -> Vec<String> {
    let setting = Setting::new(NAME, Some(&options), false);
    let check = SelectorMaxAttributeCheck::from_setting(&setting, &LintEnv::default()).unwrap();
    let max = check.max.unwrap();
    let opts = Options {
      max,
      ignore: &check.ignore,
    };
    // Exercise both counting modes directly, whatever Stylelint the test
    // run happens to find.
    let rules = scan_style_rules(source, Syntax::Scss);
    let mut out = Vec::new();
    for raw in &rules {
      let list = parse_selector_list(&raw.prelude).unwrap();
      for selector in &list.selectors {
        if v17 {
          if opts.count_deep(&selector.nodes) > max {
            out.push(check.report(raw, selector, max, true).message);
          }
        } else {
          opts.counts_by_context(selector, &mut |_| {
            out.push(check.report(raw, selector, max, false).message);
          });
        }
      }
    }
    out
  }

  const BOOTSTRAP: &str = "[list]:not([type=\"date\"]):not([type=\"datetime-local\"]):not([type=\"month\"]):not([type=\"week\"]):not([type=\"time\"])::-webkit-calendar-picker-indicator { display: none; }";

  #[test]
  fn stylelint_16_counts_each_negated_selector_on_its_own() {
    // bootstrap's `_reboot.scss`, linted by Stylelint 16 with a max of 2.
    assert!(lint(BOOTSTRAP, serde_json::json!(2), false).is_empty());
    assert_eq!(
      lint("a[b][c]:not([d]) { }", serde_json::json!(1), false),
      vec!["Expected \"a[b][c]:not([d])\" to have no more than 1 attribute selector"]
    );
    assert_eq!(
      lint("a:is([b][c][d]) { }", serde_json::json!(2), false),
      vec!["Expected \"a:is([b][c][d])\" to have no more than 2 attribute selectors"]
    );
  }

  #[test]
  fn stylelint_17_counts_every_attribute_as_written() {
    assert_eq!(
      lint(BOOTSTRAP, serde_json::json!(2), true).len(),
      1,
      "6 attribute selectors in all"
    );
    assert_eq!(
      lint(
        ".foo:has([disabled][required]) { }",
        serde_json::json!(1),
        true
      ),
      vec!["Too many attribute selectors in \".foo:has([disabled][required])\", maximum 1"]
    );
  }

  #[test]
  fn each_selector_in_a_list_is_counted_separately() {
    for v17 in [false, true] {
      assert!(lint("[type='text'], [disabled] { }", serde_json::json!(1), v17).is_empty());
    }
  }

  #[test]
  fn ignored_attributes_do_not_count() {
    let options = serde_json::json!([1, { "ignoreAttributes": ["type", "/^data-/"] }]);
    for v17 in [false, true] {
      assert!(
        lint(
          "input[type='text'][data-x][disabled] { }",
          options.clone(),
          v17
        )
        .is_empty()
      );
    }
    assert_eq!(attribute_name("[xlink|href]"), "href");
    assert_eq!(attribute_name("[ lang |= en ]"), "lang");
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    // The primary option is tested by a function, so Stylelint words its
    // problem without "value".
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([2, { "ignoreAttributes": ["type", "/^data-/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(-1)),
      ["Invalid option \"-1\" for rule \"selector-max-attribute\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([1, { "ignoreAttributes": [3] }])),
      ["Invalid value \"3\" for option \"ignoreAttributes\" of rule \"selector-max-attribute\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([1, { "ignore": ["x"] }])),
      ["Invalid option name \"ignore\" for rule \"selector-max-attribute\""]
    );
  }
}
