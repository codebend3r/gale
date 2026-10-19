use gale_css_parser::CssNode;
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
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let selector = if matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass | gale_css_parser::Syntax::Less
    ) {
      strip_scss_line_comments(&rule.selector)
    } else {
      rule.selector.clone()
    };

    let mut diags = Vec::new();
    for name in extract_type_selectors(&selector) {
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
            .span(rule.span),
        );
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

/// Strip `//` line comments from a selector string (SCSS/Less).
/// Handles both full-line comments and inline trailing comments.
fn strip_scss_line_comments(selector: &str) -> String {
  selector
    .lines()
    .map(|line| {
      // Find `//` that's not inside a string
      let mut in_single = false;
      let mut in_double = false;
      let bytes = line.as_bytes();
      let mut i = 0;
      while i < bytes.len() {
        match bytes[i] {
          b'\'' if !in_double => in_single = !in_single,
          b'"' if !in_single => in_double = !in_double,
          b'/' if !in_single && !in_double && i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
            return &line[..i];
          }
          _ => {}
        }
        i += 1;
      }
      line
    })
    .collect::<Vec<_>>()
    .join("\n")
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, StyleRule};
  use gale_diagnostics::Span;

  use crate::testing::{configured, ctx, invalid_options};

  fn style(sel: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: Span::new(0, 0),
        important: false,
      }],
      span: Span::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_unknown_type_selector() {
    let rule = configured(&SelectorTypeNoUnknown, None);
    let d = rule.check(&style("fakeelement"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("fakeelement"));
  }

  #[test]
  fn ignore_types_skips_listed_names() {
    let rule = configured(
      &SelectorTypeNoUnknown,
      serde_json::json!([true, { "ignoreTypes": ["fakeelement", "/^my/"] }]),
    );
    assert!(rule.check(&style("fakeelement"), &ctx()).is_empty());
    assert!(rule.check(&style("mything"), &ctx()).is_empty());
    assert_eq!(rule.check(&style("otherthing"), &ctx()).len(), 1);
  }

  #[test]
  fn allows_known_html_elements() {
    let rule = configured(&SelectorTypeNoUnknown, None);
    assert!(rule.check(&style("div"), &ctx()).is_empty());
    assert!(rule.check(&style("a"), &ctx()).is_empty());
    assert!(rule.check(&style("span"), &ctx()).is_empty());
  }

  #[test]
  fn skips_custom_elements() {
    let rule = configured(&SelectorTypeNoUnknown, None);
    assert!(rule.check(&style("my-component"), &ctx()).is_empty());
    assert!(rule.check(&style("app-header"), &ctx()).is_empty());
  }

  #[test]
  fn ignores_class_and_id_selectors() {
    let rule = configured(&SelectorTypeNoUnknown, None);
    assert!(rule.check(&style(".foo"), &ctx()).is_empty());
    assert!(rule.check(&style("#bar"), &ctx()).is_empty());
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
