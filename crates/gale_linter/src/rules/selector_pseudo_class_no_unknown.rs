use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::selector::postcss::blank_comments;
use gale_diagnostics::{Diagnostic, Span};

use crate::data::{is_known_pseudo_class, is_legacy_pseudo_element};
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

/// Known pseudo-classes that are only valid in a `@page` context.
/// These should NOT be recognized as valid pseudo-classes in regular selectors.
const PAGE_PSEUDO_CLASSES: &[&str] = &["blank", "first", "left", "nth", "right", "recto", "verso"];

/// Pseudo-classes that are only valid after vendor-prefixed pseudo-elements
/// (e.g., `::-webkit-scrollbar-thumb:window-inactive`).
const WEBKIT_SCROLLBAR_PSEUDO_CLASSES: &[&str] = &[
  "corner-present",
  "decrement",
  "double-button",
  "end",
  "horizontal",
  "increment",
  "no-button",
  "single-button",
  "start",
  "vertical",
  "window-inactive",
];

/// Whether `name` is a pseudo-class valid only inside `@page`.
fn is_page_pseudo_class(name: &str) -> bool {
  PAGE_PSEUDO_CLASSES
    .iter()
    .any(|p| p.eq_ignore_ascii_case(name))
}

/// Whether `name` is one of the WebKit scrollbar pseudo-classes.
fn is_webkit_scrollbar_pseudo_class(name: &str) -> bool {
  WEBKIT_SCROLLBAR_PSEUDO_CLASSES
    .iter()
    .any(|p| p.eq_ignore_ascii_case(name))
}

const NAME: &str = "selector-pseudo-class-no-unknown";

declare_rule! {
  pub struct SelectorPseudoClassNoUnknown => SelectorPseudoClassNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown pseudo-class selectors",
    default_severity: Warning,
  }
}

/// `selector-pseudo-class-no-unknown` as configured.
pub struct SelectorPseudoClassNoUnknownCheck {
  /// The `ignorePseudoClasses` option: names and `/regex/` entries.
  ignore_pseudo_classes: Matchers,
}

impl SelectorPseudoClassNoUnknownCheck {
  /// Stylelint's options: nothing but `true`, then `ignorePseudoClasses`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_pseudo_classes = options.strings("ignorePseudoClasses");
    options.finish()?;
    Ok(Self {
      ignore_pseudo_classes: Matchers::new(&ignore_pseudo_classes),
    })
  }
}

impl Check for SelectorPseudoClassNoUnknownCheck {
  /// Flags pseudo-classes that are not standard, skipping vendor-prefixed names,
  /// interpolated selectors, and anything in the ignore list.
  ///
  /// Reads every standard-syntax rule's selector and every `@page`'s params
  /// as written, and reports each pseudo-class where it stands.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let ignore_list = &self.ignore_pseudo_classes;
    let mut diags = Vec::new();
    let report = |diags: &mut Vec<Diagnostic>, start: usize, entry: &PseudoClassEntry| {
      diags.push(
        Diagnostic::new(
          NAME,
          format!(
            "Unexpected unknown pseudo-class selector \":{}\"",
            entry.name
          ),
        )
        .span(Span::new(start + entry.offset, entry.name.len() + 1)),
      );
    };

    for node in tree.walk() {
      if let Some(rule) = node.as_rule() {
        if !rule.is_standard_syntax() {
          continue;
        }
        let selector = blank_comments(&rule.raw_selector());
        for entry in &extract_pseudo_classes_with_context(&selector) {
          if entry.name.starts_with('-') {
            continue;
          }
          if is_legacy_pseudo_element(&entry.name) {
            continue;
          }
          // Webkit scrollbar pseudo-classes are valid after pseudo-elements
          if entry.after_pseudo_element && is_webkit_scrollbar_pseudo_class(&entry.name) {
            continue;
          }
          // Page pseudo-classes are NOT valid in regular selectors
          if is_page_pseudo_class(&entry.name) {
            report(&mut diags, rule.selector_start(), entry);
            continue;
          }
          if ignore_list.matches(&entry.name) {
            continue;
          }
          if !is_known_pseudo_class(&entry.name) {
            report(&mut diags, rule.selector_start(), entry);
          }
        }
      } else if let Some(at_rule) = node.as_at_rule() {
        if at_rule.name() != "page" || at_rule.params().is_empty() || !at_rule.is_standard_syntax()
        {
          continue;
        }
        let params = blank_comments(&at_rule.raw_params());
        for entry in &extract_pseudo_classes_with_context(&params) {
          if entry.name.starts_with('-') {
            continue;
          }
          if is_page_pseudo_class(&entry.name) {
            continue;
          }
          if ignore_list.matches(&entry.name) {
            continue;
          }
          if !is_known_pseudo_class(&entry.name) {
            report(&mut diags, at_rule.params_start(), entry);
          }
        }
      }
    }
    diags
  }
}

/// A pseudo-class found in a selector.
struct PseudoClassEntry {
  /// The name, without the colon.
  name: String,
  /// The byte offset of its colon in the selector.
  offset: usize,
  /// Whether a pseudo-element comes before it in its selector.
  after_pseudo_element: bool,
}

/// Extract pseudo-class names from a selector string, with context.
fn extract_pseudo_classes_with_context(selector: &str) -> Vec<PseudoClassEntry> {
  let mut classes = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let byte_offsets: Vec<usize> = selector.char_indices().map(|(at, _)| at).collect();
  let len = chars.len();
  let mut i = 0;
  let mut after_pseudo_element = false;

  while i < len {
    // An escaped character (`.md\:flex`) is part of a name, never a colon
    // that starts a pseudo-class.
    if chars[i] == '\\' {
      i += 2;
      continue;
    }
    if i + 1 < len && chars[i] == '#' && chars[i + 1] == '{' {
      i += 2;
      let mut depth = 1;
      while i < len && depth > 0 {
        if chars[i] == '{' {
          depth += 1;
        } else if chars[i] == '}' {
          depth -= 1;
        }
        i += 1;
      }
      continue;
    }

    if chars[i] == '[' {
      let mut depth = 1;
      i += 1;
      while i < len && depth > 0 {
        match chars[i] {
          '[' => depth += 1,
          ']' => depth -= 1,
          '"' | '\'' => {
            let quote = chars[i];
            i += 1;
            while i < len && chars[i] != quote {
              if chars[i] == '\\' {
                i += 1;
              }
              i += 1;
            }
          }
          _ => {}
        }
        i += 1;
      }
      continue;
    }

    if chars[i] == ',' || chars[i] == ' ' || chars[i] == '\n' || chars[i] == '\t' {
      if chars[i] == ',' {
        after_pseudo_element = false;
      }
      i += 1;
      continue;
    }

    if i + 1 < len && chars[i] == ':' && chars[i + 1] == ':' {
      i += 2;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
        i += 1;
      }
      after_pseudo_element = true;
      continue;
    }

    if chars[i] == ':' {
      let colon_pos = i;
      i += 1;
      let start = i;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
        i += 1;
      }
      if i > start {
        let name: String = chars[start..i].iter().collect();
        classes.push(PseudoClassEntry {
          name,
          offset: byte_offsets[colon_pos],
          after_pseudo_element,
        });
      }
      if i < len && chars[i] == '(' {
        let mut depth = 1;
        i += 1;
        while i < len && depth > 0 {
          if chars[i] == '(' {
            depth += 1;
          } else if chars[i] == ')' {
            depth -= 1;
          }
          i += 1;
        }
      }
    } else {
      i += 1;
    }
  }

  classes
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// A rule with the selector `sel`.
  fn style_with_selector(sel: &str) -> String {
    format!("{sel} {{ color: red; }}")
  }

  #[test]
  fn reports_unknown_pseudo_class() {
    let rule = configured(&SelectorPseudoClassNoUnknown, None);
    let d = rule.check_source(&context(&style_with_selector("a:hoverr"), Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains(":hoverr"));
  }

  #[test]
  fn allows_known_pseudo_class() {
    let rule = configured(&SelectorPseudoClassNoUnknown, None);
    assert!(
      rule
        .check_source(&context(&style_with_selector("a:hover"), Syntax::Css))
        .is_empty()
    );
    assert!(
      rule
        .check_source(&context(
          &style_with_selector("a:nth-child(2)"),
          Syntax::Css
        ))
        .is_empty()
    );
  }

  #[test]
  fn does_not_confuse_with_pseudo_elements() {
    let rule = configured(&SelectorPseudoClassNoUnknown, None);
    assert!(
      rule
        .check_source(&context(&style_with_selector("a::before"), Syntax::Css))
        .is_empty()
    );
  }

  #[test]
  fn allows_legacy_single_colon_pseudo_elements() {
    let rule = configured(&SelectorPseudoClassNoUnknown, None);
    for sel in ["p:before", "p:after", "p:first-line", "p:first-letter"] {
      assert!(
        rule
          .check_source(&context(&style_with_selector(sel), Syntax::Css))
          .is_empty(),
        "should not flag legacy pseudo-element in selector \"{sel}\"",
      );
    }
  }

  #[test]
  fn rejects_first_in_regular_selector() {
    let rule = configured(&SelectorPseudoClassNoUnknown, None);
    let d = rule.check_source(&context(&style_with_selector(":first"), Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains(":first"));
  }

  #[test]
  fn reports_each_pseudo_class_where_it_is_written() {
    let rule = configured(&SelectorPseudoClassNoUnknown, None);
    // A repeated name is reported twice, each where it stands, and a name
    // past non-ASCII text lands on its own colon.
    let source = "a:foo, \u{4e2d}:foo /* :bar */ {}\n@page :baz { margin: 0; }";
    let d = rule.check_source(&context(source, Syntax::Css));
    let found: Vec<_> = d
      .iter()
      .map(|d| (d.span.offset, &source[d.span.offset..d.span.end()]))
      .collect();
    assert_eq!(
      found,
      [
        (1, ":foo"),
        (10, ":foo"),
        (source.find(":baz").unwrap(), ":baz")
      ]
    );
  }

  #[test]
  fn an_escaped_colon_starts_no_pseudo_class() {
    let rule = configured(&SelectorPseudoClassNoUnknown, None);
    let source = ".xl\\:container, .\\32 xl\\:col-3:hover {}";
    assert!(rule.check_source(&context(source, Syntax::Css)).is_empty());
  }

  #[test]
  fn reads_rules_wherever_they_are() {
    // Rules with vendor pseudo-elements and rules nested in at-rules are
    // read like any other.
    let rule = configured(&SelectorPseudoClassNoUnknown, None);
    let source = "::-webkit-scrollbar:horizontal:nope {}\na { @media print { &:nope {} } }";
    let d = rule.check_source(&context(source, Syntax::Scss));
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignorePseudoClasses": ["hoverr", "/^my-/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"selector-pseudo-class-no-unknown\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignorePseudoClasses": [1] }])
      ),
      [
        "Invalid value \"1\" for option \"ignorePseudoClasses\" of rule \"selector-pseudo-class-no-unknown\""
      ]
    );
  }
}
