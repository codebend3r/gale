use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::{NodeRef, PostcssTree};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "nesting-selector-no-missing-scoping-root";

declare_rule! {
  /// Ensure that a nested rule using `&` always has a valid scoping root.
  ///
  /// In CSS nesting, `&` refers to the parent selector. Using `&` in a
  /// top-level rule is invalid because there is no parent context. This rule
  /// flags style rules at the document root that contain `&` in their selector.
  ///
  /// Equivalent to Stylelint's `nesting-selector-no-missing-scoping-root` rule.
  pub struct NestingSelectorNoMissingScopingRoot => NestingSelectorNoMissingScopingRootCheck {
    name: NAME,
    description: "Disallow nesting selector & without a scoping root",
    default_severity: Error,
  }
}

/// `nesting-selector-no-missing-scoping-root` as configured.
pub struct NestingSelectorNoMissingScopingRootCheck {
  /// `ignoreAtRules`: at-rule names (matched in any case) and `/regex/`
  /// entries.
  ignore_at_rules: Matchers,
}

impl NestingSelectorNoMissingScopingRootCheck {
  /// Stylelint's options: `true`, then `ignoreAtRules`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.primary_true();
    let ignore_at_rules = Matchers::ignoring_case(&options.strings("ignoreAtRules"));
    options.finish()?;
    Ok(Self { ignore_at_rules })
  }

  /// Whether the at-rule `name` is one of `ignoreAtRules`: a `/regex/`
  /// entry matches by search, a name in any case.
  fn is_ignored_at_rule(&self, name: &str) -> bool {
    self.ignore_at_rules.matches(name)
  }
}

/// Where the nesting selectors `&` sit in `selector`, as written.
///
/// Skips occurrences inside strings, attribute selectors and comments
/// (`//` ones too when `line_comments`, as in SCSS and Less).
fn nesting_offsets(selector: &str, line_comments: bool) -> Vec<usize> {
  let bytes = selector.as_bytes();
  let mut offsets = Vec::new();
  let mut i = 0;
  // Past the string starting at `i`, escapes included.
  let skip_string = |mut i: usize| {
    let quote = bytes[i];
    i += 1;
    while i < bytes.len() && bytes[i] != quote {
      if bytes[i] == b'\\' {
        i += 1;
      }
      i += 1;
    }
    i + 1
  };
  while i < bytes.len() {
    match bytes[i] {
      b'[' => {
        let mut depth = 0;
        while i < bytes.len() {
          match bytes[i] {
            b'[' => depth += 1,
            b']' => depth -= 1,
            b'"' | b'\'' => {
              i = skip_string(i);
              continue;
            }
            _ => {}
          }
          i += 1;
          if depth == 0 {
            break;
          }
        }
      }
      b'"' | b'\'' => i = skip_string(i),
      b'/' if bytes.get(i + 1) == Some(&b'*') => {
        i = selector[i + 2..]
          .find("*/")
          .map_or(bytes.len(), |end| i + 2 + end + 2);
      }
      b'/' if line_comments && bytes.get(i + 1) == Some(&b'/') => {
        i = selector[i..].find('\n').map_or(bytes.len(), |end| i + end);
      }
      b'\\' => i += 2,
      b'&' => {
        offsets.push(i);
        i += 1;
      }
      _ => i += 1,
    }
  }
  offsets
}

impl Check for NestingSelectorNoMissingScopingRootCheck {
  /// Flags each `&` in a rule's selector that has nothing to scope it: no
  /// style rule or `@scope` around the rule, nor an at-rule
  /// `ignoreAtRules` names.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let line_comments = !matches!(ctx.syntax, Syntax::Css);
    let mut diags = Vec::new();
    for rule in tree.walk_rules() {
      if !rule.is_standard_syntax() || self.has_scoping_root(rule.node()) {
        continue;
      }
      let start = rule.selector_start();
      let Some(written) = ctx.source_slice(start, rule.name_span.end) else {
        continue;
      };
      for offset in nesting_offsets(written, line_comments) {
        diags.push(
          Diagnostic::new(
            NAME,
            format!(
              "Unexpected nesting selector \"&\" without a scoping root in \"{}\"",
              rule.selector()
            ),
          )
          .span(Span::new(start + offset, 1)),
        );
      }
    }
    diags
  }
}

impl NestingSelectorNoMissingScopingRootCheck {
  /// Stylelint's `hasValidScopingRoot`: whether a style rule, an `@scope`
  /// or an ignored at-rule encloses `node`.
  fn has_scoping_root(&self, node: NodeRef) -> bool {
    node.ancestors().any(|ancestor| {
      ancestor.as_rule().is_some()
        || ancestor
          .as_at_rule()
          .is_some_and(|at| at.name() == "scope" || self.is_ignored_at_rule(at.name()))
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// The selectors reported for `source` in `syntax` with `options`.
  fn reported(source: &str, syntax: Syntax, options: Option<serde_json::Value>) -> Vec<String> {
    configured(&NestingSelectorNoMissingScopingRoot, options)
      .check_source(&context(source, syntax))
      .into_iter()
      .map(|d| d.message)
      .collect()
  }

  /// The message for a rule with `selector`.
  fn message(selector: &str) -> String {
    format!("Unexpected nesting selector \"&\" without a scoping root in \"{selector}\"")
  }

  #[test]
  fn reports_top_level_nesting_selector() {
    assert_eq!(
      reported("& .child { color: red }", Syntax::Css, None),
      [message("& .child")]
    );
  }

  #[test]
  fn reports_top_level_ampersand_alone() {
    let d = configured(&NestingSelectorNoMissingScopingRoot, None)
      .check_source(&context("& { color: red }", Syntax::Css));
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].span.offset, d[0].span.length), (0, 1));
  }

  #[test]
  fn reports_each_nesting_selector_where_it_is_written() {
    let source = "a, /* & */ [x='&'] &, :is(&) & { color: red }";
    let offsets: Vec<usize> = configured(&NestingSelectorNoMissingScopingRoot, None)
      .check_source(&context(source, Syntax::Css))
      .into_iter()
      .map(|d| d.span.offset)
      .collect();
    assert_eq!(offsets, [19, 26, 29]);
    let source = "a, // &\n& {}";
    let d = reported(source, Syntax::Scss, None);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_normal_top_level_selector() {
    assert!(reported(".a .b { color: red }", Syntax::Css, None).is_empty());
  }

  #[test]
  fn allows_nested_nesting_selector() {
    let source = ".parent { & .child { color: red } @media print { & { top: 0 } } }";
    assert!(reported(source, Syntax::Css, None).is_empty());
  }

  #[test]
  fn reports_inside_at_rules_without_a_parent_rule() {
    let source = "@media (min-width: 768px) { & .x {} @supports (x: y) { &:hover {} } }";
    assert_eq!(
      reported(source, Syntax::Css, None),
      [message("& .x"), message("&:hover")]
    );
  }

  #[test]
  fn scope_is_a_scoping_root() {
    assert!(reported("@scope (.card) { & .title {} }", Syntax::Css, None).is_empty());
  }

  #[test]
  fn skips_non_standard_selectors() {
    assert!(reported("&.#{$class} .source {}", Syntax::Scss, None).is_empty());
  }

  #[test]
  fn allows_no_ampersand_inside_at_rule() {
    assert!(reported("@media print { .a {} }", Syntax::Css, None).is_empty());
  }

  #[test]
  fn ignores_at_rule_when_listed_in_ignore_at_rules() {
    let options = serde_json::json!([true, { "ignoreAtRules": ["mixin"] }]);
    let source = "@mixin onebox($class) { & .source {} }";
    assert!(reported(source, Syntax::Scss, Some(options.clone())).is_empty());
    // Other at-rules still report.
    assert_eq!(
      reported(
        "@layer utilities { & .child {} }",
        Syntax::Scss,
        Some(options)
      ),
      [message("& .child")]
    );
  }

  #[test]
  fn ignore_at_rules_is_case_insensitive() {
    let options = serde_json::json!([true, { "ignoreAtRules": ["mixin"] }]);
    assert!(reported("@Mixin foo { & .bar {} }", Syntax::Scss, Some(options)).is_empty());
  }

  #[test]
  fn ignore_at_rules_takes_regexes() {
    let options = serde_json::json!([true, { "ignoreAtRules": ["/^mix/"] }]);
    assert!(reported("@mixin foo { & .bar {} }", Syntax::Scss, Some(options)).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreAtRules": ["mixin", "/^incl/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Invalid option value \"always\" for rule \"nesting-selector-no-missing-scoping-root\""]
    );
  }
}
