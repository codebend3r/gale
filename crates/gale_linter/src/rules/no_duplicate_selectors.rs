use std::collections::HashMap;

use gale_css_parser::postcss_tree::{AtRuleRef, NodeRef, PostcssTree};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::nested_selectors::{SelectorResolver, normalize_selector};
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "no-duplicate-selectors";

declare_rule! {
  /// Disallow duplicate selectors within a stylesheet.
  ///
  /// Equivalent to Stylelint's `no-duplicate-selectors` rule.
  pub struct NoDuplicateSelectors => NoDuplicateSelectorsCheck {
    name: NAME,
    description: "Disallow duplicate selectors within a stylesheet",
    default_severity: Warning,
  }
}

/// `no-duplicate-selectors` as configured.
pub struct NoDuplicateSelectorsCheck {
  /// `ignoreSelectors`: selectors (names or `/regex/` entries) that may
  /// repeat.
  ignore_selectors: Matchers,
}

impl NoDuplicateSelectorsCheck {
  /// Stylelint's options: `true`, then `disallowInList` and
  /// `ignoreSelectors`.  Gale accepts but does not yet apply
  /// `disallowInList`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.flag("disallowInList");
    let ignore_selectors = options.matchers("ignoreSelectors");
    options.finish()?;
    Ok(Self { ignore_selectors })
  }
}

impl Check for NoDuplicateSelectorsCheck {
  /// Flags a selector list that repeats one seen earlier in the same scope
  /// (the root or the nearest at-rule), once both are resolved against
  /// their ancestors as CSS nesting does, reporting the first occurrence's
  /// line and skipping anything in `ignoreSelectors`.
  fn check(&self, tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    let mut resolver = SelectorResolver::default();
    // The selector lists seen in each scope, with their first lines.
    let mut seen: HashMap<(Option<usize>, String), (usize, Span)> = HashMap::new();
    let mut diagnostics = Vec::new();
    for rule in tree.walk_rules() {
      // Unlike Stylelint's `isKeyframeRule`, vendor-prefixed `@keyframes`
      // count too.
      let in_keyframes = rule
        .parent()
        .and_then(NodeRef::as_at_rule)
        .is_some_and(AtRuleRef::is_keyframes);
      if in_keyframes || !rule.is_standard_syntax() {
        continue;
      }
      let Some(resolved) = resolver.resolve(rule) else {
        continue;
      };
      let normalized: Vec<String> = resolved.selectors.iter().map(normalize_selector).collect();
      if normalized.is_empty()
        || (!self.ignore_selectors.is_empty()
          && normalized.iter().any(|s| self.ignore_selectors.matches(s)))
      {
        continue;
      }
      // The list compares as a whole, in any order and without repeats.
      let mut key = normalized;
      key.sort_unstable();
      key.dedup();
      let scope = rule
        .ancestors()
        .find(|node| node.as_at_rule().is_some())
        .map(NodeRef::id);
      match seen.get(&(scope, key.join(","))) {
        Some(&(first_line, first_span)) => {
          let first_line = first_line + context.embedded.map_or(0, |root| root.lines_before());
          let written = rule.raw_selector();
          diagnostics.push(
            Diagnostic::new(
              NAME,
              format!(
                "Unexpected duplicate selector \"{}\", first used at line {first_line}",
                written.trim(),
              ),
            )
            .span(Span::from_range(rule.selector_start(), rule.name_span.end))
            .related(first_span, "First occurrence of this selector"),
          );
        }
        None => {
          seen.insert(
            (scope, key.join(",")),
            (
              rule.start_line(),
              Span::from_range(rule.selector_start(), rule.name_span.end),
            ),
          );
        }
      }
    }
    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context};

  /// What the rule reports for `source` in `syntax` with `options`: each
  /// message and the text it points at.
  fn check(
    source: &str,
    syntax: Syntax,
    options: Option<serde_json::Value>,
  ) -> Vec<(String, &str)> {
    configured(&NoDuplicateSelectors, options)
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

  #[test]
  fn reports_duplicate_selectors_with_line_reference() {
    let source = ".foo { color: red; }\n.bar { color: blue; }\n.foo { display: block; }";
    for syntax in [Syntax::Css, Syntax::Scss, Syntax::Less] {
      assert_eq!(
        check(source, syntax, None),
        [(
          "Unexpected duplicate selector \".foo\", first used at line 1".to_string(),
          ".foo"
        )]
      );
    }
  }

  #[test]
  fn ignores_unique_selectors() {
    assert!(check(".foo {}\n.bar {}", Syntax::Css, None).is_empty());
  }

  #[test]
  fn lists_compare_normalized_in_any_order() {
    assert_eq!(check("a, b {}\nb,\n  a {}", Syntax::Css, None).len(), 1);
    assert_eq!(
      check("a>b.c.d {}\na > b.d.c {}", Syntax::Css, None).len(),
      1
    );
    assert_eq!(
      check("[ x = 'y' ] {}\n[x='y'] {}", Syntax::Css, None).len(),
      1
    );
  }

  #[test]
  fn scoped_selectors_in_at_rules_dont_clash() {
    let source = ".foo {}\n@media (min-width: 768px) { .foo {} }\n@media print { .foo {} }";
    assert!(check(source, Syntax::Css, None).is_empty());
  }

  #[test]
  fn skips_keyframe_selectors_and_placeholders() {
    assert!(check("@keyframes k { from {} from {} }", Syntax::Css, None).is_empty());
    assert!(check("%a {}\n%a {}", Syntax::Scss, None).is_empty());
  }

  #[test]
  fn nested_selectors_resolve_as_css_nesting_does() {
    // Each `& {}` resolves to `.parent`, which duplicates the parent rule.
    let source = ".parent {\n  & { color: red; }\n  & { color: blue; }\n}";
    assert_eq!(check(source, Syntax::Scss, None).len(), 2);
    let source = "a { b {} }\na b {}\nc { @media print { & b {} } }";
    assert_eq!(
      check(source, Syntax::Css, None),
      [(
        "Unexpected duplicate selector \"a b\", first used at line 1".to_string(),
        "a b"
      )]
    );
    // A parent that is not one compound selector is wrapped in `:is()`, so
    // these two are written differently and do not match.
    let source = ".a .b { &:hover {} }\n.a .b:hover {}";
    assert!(check(source, Syntax::Css, None).is_empty());
  }

  #[test]
  fn ignore_selectors_string_match() {
    let source = ".foo { color: red; }\n.bar { color: blue; }\n.foo { display: block; }";
    let options = serde_json::json!([true, {"ignoreSelectors": [".foo"]}]);
    assert!(check(source, Syntax::Css, Some(options)).is_empty());
  }

  #[test]
  fn ignore_selectors_regex_match() {
    let source = ".foo { color: red; }\n.foo { display: block; }";
    let options = serde_json::json!([true, {"ignoreSelectors": ["/^\\.foo$/"]}]);
    assert!(check(source, Syntax::Css, Some(options)).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "disallowInList": true, "ignoreSelectors": ".foo" }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "disallowInList": "yes" }])),
      ["Invalid value \"yes\" for option \"disallowInList\" of rule \"no-duplicate-selectors\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignore": [".foo"] }])),
      ["Invalid option name \"ignore\" for rule \"no-duplicate-selectors\""]
    );
  }

  #[test]
  fn ignore_selectors_takes_a_single_string() {
    use crate::testing::lint;
    let source = ".foo {}\n.foo {}";
    let ignore = serde_json::json!([true, { "ignoreSelectors": ".foo" }]);
    assert!(lint(NAME, ignore, source, Syntax::Css).is_empty());
  }
}
