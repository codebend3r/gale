use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/declaration-nested-properties";

declare_rule! {
  /// Require or disallow properties nested in SCSS namespace declarations.
  ///
  /// SCSS allows nesting properties by namespace:
  ///
  /// ```scss
  /// // Nested properties
  /// font: {
  ///   weight: bold;
  ///   size: 1em;
  /// }
  /// ```
  ///
  /// Default: `"never"` — disallow nested properties.
  ///
  /// Equivalent to `scss/declaration-nested-properties`.
  pub struct ScssDeclarationNestedProperties => ScssDeclarationNestedPropertiesCheck {
    name: NAME,
    description: "Require or disallow nested properties in SCSS",
    default_severity: Warning,
  }
}

/// `scss/declaration-nested-properties` as configured.
pub struct ScssDeclarationNestedPropertiesCheck {
  /// The primary option: `"always"` or `"never"`.
  expectation: &'static str,
}

impl ScssDeclarationNestedPropertiesCheck {
  /// Stylelint's options: `"always"` or `"never"` (`"never"` when not
  /// given), then `except: ["only-of-namespace"]`.  Gale implements only
  /// `"never"`; `"always"` and `except` (which only affects `"always"`) are
  /// accepted but not yet applied.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expectation = options
      .primary_one_of(&["always", "never"])
      .unwrap_or("never");
    options.keywords("except", &["only-of-namespace"]);
    options.finish()?;
    Ok(Self { expectation })
  }
}

impl Check for ScssDeclarationNestedPropertiesCheck {
  /// Under "never", flags SCSS namespace blocks such as `font: { … }`.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    if self.expectation != "never" {
      return vec![];
    }

    let source = ctx.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();

    // Scan for patterns like `property-namespace: {` which indicates nested
    // properties. This is a simplified detection that looks for `: {` patterns
    // where the colon is preceded by a CSS property name.
    let mut i = 0;
    while i < len {
      if bytes[i] == b':' {
        // Check what's before the colon (should be a property-like identifier)
        let mut before = i;
        before = before.saturating_sub(1);
        while before > 0 && (bytes[before] == b' ' || bytes[before] == b'\t') {
          before -= 1;
        }
        let is_property_like =
          before < i && (bytes[before].is_ascii_alphabetic() || bytes[before] == b'-');

        // Check what's after the colon (skip whitespace, look for `{`)
        let mut after = i + 1;
        while after < len && (bytes[after] == b' ' || bytes[after] == b'\t') {
          after += 1;
        }

        if is_property_like && after < len && bytes[after] == b'{' {
          // This looks like a nested property declaration
          // Find the start of the property name
          let mut prop_start = before;
          while prop_start > 0
            && (bytes[prop_start - 1].is_ascii_alphanumeric() || bytes[prop_start - 1] == b'-')
          {
            prop_start -= 1;
          }

          // Make sure we're not inside a selector (check if preceded by
          // whitespace/newline/semicolon/brace, not another property context)
          if prop_start == 0
            || matches!(
              bytes[prop_start - 1],
              b'\n' | b'\r' | b';' | b'{' | b' ' | b'\t'
            )
          {
            diagnostics.push(
              Diagnostic::new(NAME, "Unexpected nested property declaration".to_string())
                .span(Span::new(prop_start, after - prop_start + 1)),
            );
          }
        }
      }
      i += 1;
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{configured, ctx_with_source, invalid_options, scss_ctx_with_source};

  #[test]
  fn reports_nested_properties() {
    let rule = configured(&ScssDeclarationNestedProperties, None);
    let ctx = scss_ctx_with_source(".foo {\n  font: {\n    weight: bold;\n  }\n}");
    let d = rule.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_regular_declarations() {
    let rule = configured(&ScssDeclarationNestedProperties, None);
    let ctx = scss_ctx_with_source(".foo {\n  font-weight: bold;\n}");
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssDeclarationNestedProperties, None);
    let ctx = ctx_with_source(".foo { font: { weight: bold; } }");
    assert!(rule.check_root(&[], &ctx).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sideways")),
      ["Invalid option value \"sideways\" for rule \"scss/declaration-nested-properties\""]
    );
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["always", { "except": ["only-of-namespace"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["never", { "except": ["first-nested"] }])
      ),
      [
        "Invalid value \"first-nested\" for option \"except\" of rule \"scss/declaration-nested-properties\""
      ]
    );
  }
}
