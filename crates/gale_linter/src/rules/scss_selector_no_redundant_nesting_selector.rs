use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/selector-no-redundant-nesting-selector";

declare_rule! {
  /// Disallow redundant nesting selectors (`&`).
  ///
  /// Reports when `&` appears at the start of a nested selector followed by a
  /// combinator or descendant selector, because the `&` can be removed without
  /// changing the meaning.
  ///
  /// A standalone `& { ... }` is **not** flagged — it serves as a scoping
  /// mechanism (e.g. after `@include` or `@extend`).
  ///
  /// ```scss
  /// // Bad
  /// .foo {
  ///   & .bar { color: red; }   // → `.bar { color: red; }`
  ///   & > .bar { color: red; } // → `> .bar { color: red; }`
  /// }
  ///
  /// // Good
  /// .foo {
  ///   & { color: red; }        // scoping block
  ///   &:hover { color: red; }  // pseudo-class append
  ///   &.bar { color: red; }    // class append
  ///   &__element { ... }       // BEM concatenation
  /// }
  /// ```
  ///
  /// Equivalent to `scss/selector-no-redundant-nesting-selector`.
  pub struct ScssSelectorNoRedundantNestingSelector => ScssSelectorNoRedundantNestingSelectorCheck {
    name: NAME,
    description: "Disallow redundant nesting selectors (&)",
    default_severity: Warning,
  }
}

/// `scss/selector-no-redundant-nesting-selector` as configured.
pub struct ScssSelectorNoRedundantNestingSelectorCheck {
  /// `ignoreKeywords`: a type selector after the `&` and its combinator
  /// that matches one of these (names or `/regex/`es) keeps the `&`: it is
  /// a keyword (such as Less's `when`) rather than a tag.
  ignore_keywords: Matchers,
}

impl ScssSelectorNoRedundantNestingSelectorCheck {
  /// Stylelint's options: `true`, then `ignoreKeywords`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_keywords = options.matchers("ignoreKeywords");
    options.finish()?;
    Ok(Self { ignore_keywords })
  }

  /// Whether the selector after a leading `& ` (`after`, trimmed) starts
  /// with a type selector that `ignoreKeywords` lists, past an explicit
  /// combinator if there is one.
  fn is_ignored_keyword(&self, after: &str) -> bool {
    if self.ignore_keywords.is_empty() {
      return false;
    }
    let compound = after
      .strip_prefix(['>', '+', '~'])
      .map_or(after, str::trim_start);
    let tag_len = compound
      .find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_' || !c.is_ascii()))
      .unwrap_or(compound.len());
    let tag = &compound[..tag_len];
    let starts_like_a_tag = tag
      .chars()
      .next()
      .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '-' || !c.is_ascii());
    starts_like_a_tag && self.ignore_keywords.matches(tag)
  }
}

impl Check for ScssSelectorNoRedundantNestingSelectorCheck {
  /// Flags a nested selector whose leading `&` is followed by a combinator or
  /// descendant, where dropping it changes nothing.
  ///
  /// Every rule is read, as stylelint-scss reads them, wherever it sits.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let mut diags = Vec::new();

    // Check nested children for redundant `&` selectors.
    //
    // A standalone `& { ... }` (where the entire selector is just `&`) is NOT
    // flagged.  In SCSS it serves as a scoping mechanism (e.g. after `@include`
    // or `@extend`), and Stylelint's scss/selector-no-redundant-nesting-selector
    // does not flag it either.
    //
    // We only flag `&` when it appears at the **start** of a compound selector
    // followed by a combinator or another simple selector — i.e. the `&` can be
    // removed without changing the meaning:
    //   `& .foo`  → `.foo`
    //   `& > .bar` → `> .bar`
    //   `& + .baz` → `+ .baz`
    //   `& ~ .qux` → `~ .qux`
    for child in tree.walk_rules() {
      // Split selector list by commas and check each individual selector.
      // This handles cases like `.other, & .child` where only one part of
      // the comma-separated list contains a redundant `&`.
      //
      // We report one diagnostic per redundant `&` selector part, with
      // the span pointing to the `&` character in the source.  To find
      // the byte offset of each part we track our position through the
      // original selector string.
      // The selector as written, comments and all, so offsets into it are
      // offsets into the source.
      let full_selector = &tree.source()[child.name_span.clone()];
      let mut search_start: usize = 0;

      for selector_part in full_selector.split(',') {
        let part_len = selector_part.len();
        // Offset of this part within `full_selector`.
        let part_offset_in_selector = search_start;
        // Move past this part + the comma delimiter.
        search_start += part_len + 1; // +1 for the comma

        let selector = selector_part.trim();
        let trim_offset = selector_part.len() - selector_part.trim_start().len();

        // Skip standalone `&` — it's a scoping block, not redundant.
        if selector == "&" {
          continue;
        }

        if let Some(rest) = selector.strip_prefix('&') {
          let redundant = if rest.starts_with(char::is_whitespace) {
            let after = rest.trim_start();
            !after.is_empty() && !self.is_ignored_keyword(after)
          } else {
            false
          };
          if redundant {
            // Point the span at the `&` character in the source.
            let ampersand_offset = child.selector_start() + part_offset_in_selector + trim_offset;
            diags.push(
              Diagnostic::new(NAME, "Unnecessary nesting selector (&)".to_string())
                .span(Span::new(ampersand_offset, 1)),
            );
          }
        }
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{
    CheckSource, configured, ctx_with_source, invalid_options, scss_ctx_with_source,
  };

  /// A rule `parent_sel` holding a rule `child_sel`.
  fn nested_rule(parent_sel: &str, child_sel: &str) -> String {
    format!("{parent_sel} {{ {child_sel} {{ color: red; }} }}")
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssSelectorNoRedundantNestingSelector, None);
    let node = nested_rule(".foo", "&");
    assert!(rule.check_source(&ctx_with_source(&node)).is_empty());
  }

  #[test]
  fn allows_standalone_ampersand_block() {
    let rule = configured(&ScssSelectorNoRedundantNestingSelector, None);
    // A bare `& { ... }` is a scoping mechanism and should NOT be flagged.
    let node = nested_rule(".foo", "&");
    assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
  }

  #[test]
  fn reports_redundant_ampersand_descendant() {
    let rule = configured(&ScssSelectorNoRedundantNestingSelector, None);
    // `& .bar` is redundant — can be written as `.bar`.
    let node = nested_rule(".foo", "& .bar");
    let d = rule.check_source(&scss_ctx_with_source(&node));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unnecessary nesting"));
  }

  #[test]
  fn reports_redundant_ampersand_child_combinator() {
    let rule = configured(&ScssSelectorNoRedundantNestingSelector, None);
    // `& > .bar` is redundant — can be written as `> .bar`.
    let node = nested_rule(".foo", "& > .bar");
    let d = rule.check_source(&scss_ctx_with_source(&node));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unnecessary nesting"));
  }

  #[test]
  fn reports_redundant_ampersand_sibling_combinator() {
    let rule = configured(&ScssSelectorNoRedundantNestingSelector, None);
    // `& + .bar` is redundant — can be written as `+ .bar`.
    let node = nested_rule(".foo", "& + .bar");
    let d = rule.check_source(&scss_ctx_with_source(&node));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unnecessary nesting"));
  }

  #[test]
  fn allows_ampersand_with_pseudo() {
    let rule = configured(&ScssSelectorNoRedundantNestingSelector, None);
    let node = nested_rule(".foo", "&:hover");
    assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
  }

  #[test]
  fn allows_ampersand_with_class() {
    let rule = configured(&ScssSelectorNoRedundantNestingSelector, None);
    let node = nested_rule(".foo", "&.bar");
    assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
  }

  #[test]
  fn allows_ampersand_concatenation() {
    let rule = configured(&ScssSelectorNoRedundantNestingSelector, None);
    // `&__element` is BEM-style concatenation, NOT redundant.
    let node = nested_rule(".foo", "&__element");
    assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
  }

  #[test]
  fn allows_non_ampersand_selector() {
    let rule = configured(&ScssSelectorNoRedundantNestingSelector, None);
    let node = nested_rule(".foo", ".bar");
    assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
  }

  #[test]
  fn reports_redundant_ampersand_in_selector_list() {
    let rule = configured(&ScssSelectorNoRedundantNestingSelector, None);
    // `.other, & .child` — the `& .child` part is redundant.
    let node = nested_rule(".foo", ".other, & .child");
    let d = rule.check_source(&scss_ctx_with_source(&node));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unnecessary nesting"));
  }

  #[test]
  fn reports_redundant_ampersand_mixed_with_valid() {
    let rule = configured(&ScssSelectorNoRedundantNestingSelector, None);
    // `&:hover, & .child` — `&:hover` is fine but `& .child` is redundant.
    let node = nested_rule(".foo", "&:hover, & .child");
    let d = rule.check_source(&scss_ctx_with_source(&node));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unnecessary nesting"));
  }

  #[test]
  fn allows_selector_list_with_no_redundant_parts() {
    let rule = configured(&ScssSelectorNoRedundantNestingSelector, None);
    // `&:hover, &.active` — neither part is redundant.
    let node = nested_rule(".foo", "&:hover, &.active");
    assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
  }

  #[test]
  fn reports_all_redundant_parts_in_selector_list() {
    let rule = configured(&ScssSelectorNoRedundantNestingSelector, None);
    // Both `& .child1` and `& .child2` are redundant — report 2 diagnostics.
    let node = nested_rule(".foo", "& .child1, & .child2");
    let d = rule.check_source(&scss_ctx_with_source(&node));
    assert_eq!(d.len(), 2);
    assert!(d[0].message.contains("Unnecessary nesting"));
    assert!(d[1].message.contains("Unnecessary nesting"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreKeywords": ["when", "/^el/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      [
        "Unexpected option value \"always\" for rule \"scss/selector-no-redundant-nesting-selector\""
      ]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignoreKeywords": [1] }])),
      [
        "Invalid value \"1\" for option \"ignoreKeywords\" of rule \"scss/selector-no-redundant-nesting-selector\""
      ]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignore": ["x"] }])),
      ["Invalid option name \"ignore\" for rule \"scss/selector-no-redundant-nesting-selector\""]
    );
  }

  #[test]
  fn ignore_keywords_keeps_the_nesting_selector() {
    let source =
      ".a {\n  & when { color: red; }\n  & > element { color: red; }\n  & .b { color: red; }\n}\n";
    let lint = |options: serde_json::Value| {
      crate::testing::lint(NAME, options, source, gale_css_parser::Syntax::Scss).len()
    };
    assert_eq!(lint(serde_json::json!(true)), 3);
    assert_eq!(
      lint(serde_json::json!([true, { "ignoreKeywords": ["when", "/^el/"] }])),
      1
    );
  }

  #[test]
  fn points_at_each_ampersand_as_written() {
    let rule = configured(&ScssSelectorNoRedundantNestingSelector, None);
    let source = ".a {\n  // x, y\n  .b,\n  & .c { color: red; }\n}";
    let d = rule.check_source(&scss_ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].span, Span::new(source.find("& .c").unwrap(), 1));
  }

  #[test]
  fn reads_rules_wherever_they_sit() {
    // stylelint-scss walks every rule: those in a nested at-rule, in a
    // mixin and at the top level too.
    let rule = configured(&ScssSelectorNoRedundantNestingSelector, None);
    let source = ".a { @media print { & .b {} } }\n@mixin m { & .c {} }\n& .d {}";
    assert_eq!(rule.check_source(&scss_ctx_with_source(source)).len(), 3);
  }
}
