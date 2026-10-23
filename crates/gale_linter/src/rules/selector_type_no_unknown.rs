use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::data::is_known_html_element;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "selector-type-no-unknown";

declare_rule! {
  /// Reports unknown type selectors (HTML element names).
  ///
  /// Custom elements (containing a hyphen like `my-component`) are skipped.
  ///
  /// Equivalent to Stylelint's `selector-type-no-unknown` rule.
  pub struct SelectorTypeNoUnknown => SelectorTypeNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown type selectors",
    default_severity: Warning,
  }
}

/// `selector-type-no-unknown` as configured.
pub struct SelectorTypeNoUnknownCheck {
  /// The `ignoreTypes` option.
  ignore_types: Matchers,
}

impl SelectorTypeNoUnknownCheck {
  /// Stylelint's options: nothing but `true`, then `ignore`
  /// (`"custom-elements"`, `"default-namespace"`), `ignoreNamespaces` and
  /// `ignoreTypes`.  Gale applies `ignoreTypes`; it always passes over
  /// custom elements, and accepts the rest without applying them yet.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.keywords("ignore", &["custom-elements", "default-namespace"]);
    options.strings("ignoreNamespaces");
    let ignore_types = options.matchers("ignoreTypes");
    options.finish()?;
    Ok(Self { ignore_types })
  }
}

impl Check for SelectorTypeNoUnknownCheck {
  /// Flags type selectors that are not known HTML elements. Custom elements
  /// and preprocessor line comments are skipped.
  ///
  /// The selector is read as PostCSS stores it, without its comments, and
  /// rules that are not standard syntax (a Less mixin guard's `when`) are
  /// skipped, as in Stylelint.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for rule in tree.walk_rules().filter(|rule| rule.is_standard_syntax()) {
      for name in extract_type_selectors(rule.selector()) {
        // Skip custom elements (contain a hyphen)
        if name.contains('-') {
          continue;
        }
        // Skip keyframe selectors (`from`, `to`).
        if name.eq_ignore_ascii_case("from") || name.eq_ignore_ascii_case("to") {
          continue;
        }
        if self.ignore_types.matches(&name) {
          continue;
        }
        if !is_known_html_element(&name) {
          diags.push(
            Diagnostic::new(NAME, format!("Unexpected unknown type selector \"{name}\""))
              .span(rule.span()),
          );
        }
      }
    }
    diags
  }
}

/// Extract type selectors (bare element names) from a selector string.
///
/// A type selector is a bare identifier at the start of a selector or after
/// a combinator (space, >, +, ~, ,). We skip IDs (#), classes (.),
/// attributes ([), pseudo-classes (:), and pseudo-elements (::).
fn extract_type_selectors(selector: &str) -> Vec<String> {
  let mut types = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  // Whether we expect a type selector at the current position
  let mut expect_type = true;

  while i < len {
    let ch = chars[i];

    match ch {
      // Skip block comments (/* ... */)
      '/' if i + 1 < len && chars[i + 1] == '*' => {
        i += 2;
        while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
          i += 1;
        }
        if i + 1 < len {
          i += 2; // skip closing */
        }
        expect_type = true;
        continue;
      }
      // Skip pseudo-elements (::name)
      ':' if i + 1 < len && chars[i + 1] == ':' => {
        i += 2;
        while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
          i += 1;
        }
        expect_type = false;
        continue;
      }
      // Skip pseudo-classes (:name) and their arguments
      ':' => {
        i += 1;
        while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
          i += 1;
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
        expect_type = false;
        continue;
      }
      // Skip class selectors (.name)
      '.' => {
        i += 1;
        while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
          i += 1;
        }
        expect_type = false;
        continue;
      }
      // Skip ID selectors (#name)
      '#' => {
        i += 1;
        while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
          i += 1;
        }
        expect_type = false;
        continue;
      }
      // Skip attribute selectors ([...])
      '[' => {
        let mut depth = 1;
        i += 1;
        while i < len && depth > 0 {
          if chars[i] == '[' {
            depth += 1;
          } else if chars[i] == ']' {
            depth -= 1;
          }
          i += 1;
        }
        expect_type = false;
        continue;
      }
      // Combinators reset to expecting a type selector
      '>' | '+' | '~' | ',' => {
        i += 1;
        expect_type = true;
        continue;
      }
      // Universal selector
      '*' => {
        i += 1;
        expect_type = false;
        continue;
      }
      // The & nesting selector
      '&' => {
        i += 1;
        expect_type = false;
        continue;
      }
      // Whitespace acts as descendant combinator
      c if c.is_ascii_whitespace() => {
        i += 1;
        expect_type = true;
        continue;
      }
      // Identifier character — could be a type selector
      c if c.is_ascii_alphabetic() && expect_type => {
        let start = i;
        while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
          i += 1;
        }
        let name: String = chars[start..i].iter().collect();
        types.push(name);
        expect_type = false;
        continue;
      }
      _ => {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
          // Skip over identifiers we don't care about
          while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_')
          {
            i += 1;
          }
        } else {
          i += 1;
        }
        expect_type = false;
        continue;
      }
    }
  }

  types
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// A rule with the selector `sel`.
  fn style(sel: &str) -> String {
    format!("{sel} {{ color: red; }}")
  }

  #[test]
  fn reports_unknown_type_selector() {
    let rule = configured(&SelectorTypeNoUnknown, None);
    let d = rule.check_source(&context(&style("fakeelement"), Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("fakeelement"));
  }

  #[test]
  fn ignore_types_skips_listed_names() {
    let rule = configured(
      &SelectorTypeNoUnknown,
      serde_json::json!([true, { "ignoreTypes": ["fakeelement", "/^my/"] }]),
    );
    assert!(
      rule
        .check_source(&context(&style("fakeelement"), Syntax::Css))
        .is_empty()
    );
    assert!(
      rule
        .check_source(&context(&style("mything"), Syntax::Css))
        .is_empty()
    );
    assert_eq!(
      rule
        .check_source(&context(&style("otherthing"), Syntax::Css))
        .len(),
      1
    );
  }

  #[test]
  fn allows_known_html_elements() {
    let rule = configured(&SelectorTypeNoUnknown, None);
    assert!(
      rule
        .check_source(&context(&style("div"), Syntax::Css))
        .is_empty()
    );
    assert!(
      rule
        .check_source(&context(&style("a"), Syntax::Css))
        .is_empty()
    );
    assert!(
      rule
        .check_source(&context(&style("span"), Syntax::Css))
        .is_empty()
    );
  }

  #[test]
  fn skips_custom_elements() {
    let rule = configured(&SelectorTypeNoUnknown, None);
    assert!(
      rule
        .check_source(&context(&style("my-component"), Syntax::Css))
        .is_empty()
    );
    assert!(
      rule
        .check_source(&context(&style("app-header"), Syntax::Css))
        .is_empty()
    );
  }

  #[test]
  fn ignores_class_and_id_selectors() {
    let rule = configured(&SelectorTypeNoUnknown, None);
    assert!(
      rule
        .check_source(&context(&style(".foo"), Syntax::Css))
        .is_empty()
    );
    assert!(
      rule
        .check_source(&context(&style("#bar"), Syntax::Css))
        .is_empty()
    );
  }

  #[test]
  fn skips_non_standard_rules_and_keyframe_selectors() {
    let rule = configured(&SelectorTypeNoUnknown, None);
    let less = ".m() when (iscolor(@c)) { color: @c; }\n.n(@a) when (@a > 0) {}";
    assert!(rule.check_source(&context(less, Syntax::Less)).is_empty());
    let css = "@keyframes k { from {} 50% {} to {} }";
    assert!(rule.check_source(&context(css, Syntax::Css)).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!([true, { "ignore": ["custom-elements", "default-namespace"], "ignoreNamespaces": ["/^svg/"], "ignoreTypes": ["x"] }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"selector-type-no-unknown\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignore": ["types"] }])),
      ["Invalid value \"types\" for option \"ignore\" of rule \"selector-type-no-unknown\""]
    );
  }
}
