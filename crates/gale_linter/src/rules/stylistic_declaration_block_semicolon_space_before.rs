use gale_css_parser::postcss_tree::{DeclRef, PostcssTree};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/declaration-block-semicolon-space-before";

declare_rule! {
  /// Require or disallow a space before the semicolons of declaration blocks.
  ///
  /// Equivalent to `@stylistic/declaration-block-semicolon-space-before`.
  pub struct StylisticDeclarationBlockSemicolonSpaceBefore => StylisticDeclarationBlockSemicolonSpaceBeforeCheck {
    name: NAME,
    description: "Require or disallow a space before the semicolons of declaration blocks",
    default_severity: Warning,
  }
}

/// `@stylistic/declaration-block-semicolon-space-before` as configured.
pub struct StylisticDeclarationBlockSemicolonSpaceBeforeCheck {
  /// The primary option: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`.
  option: &'static str,
}

impl StylisticDeclarationBlockSemicolonSpaceBeforeCheck {
  /// The plugin's options: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`.  Gale reads no option as `"never"`.
  /// `"always-single-line"` and `"never-single-line"` are accepted but not yet
  /// applied.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option =
      options.primary_one_of(&["always", "never", "always-single-line", "never-single-line"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("never"),
    })
  }
}

impl Check for StylisticDeclarationBlockSemicolonSpaceBeforeCheck {
  /// Flags a declaration semicolon whose preceding space does not match the option.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let message = match self.option {
      "always" => "Expected single space before \";\"",
      "never" => "Unexpected whitespace before \";\"",
      _ => return vec![],
    };
    let source = ctx.source.as_bytes();
    tree
      .walk_decls()
      .filter(|decl| decl.parent_is_rule())
      // The plugin lets `--x: ;` (a custom property set to a space) keep
      // its space under "never".
      .filter(|decl| !(self.option == "never" && is_space_custom_property(*decl)))
      .filter_map(DeclRef::semicolon)
      .filter(|&semi| {
        let has_space = matches!(source[semi - 1], b' ' | b'\t');
        has_space != (self.option == "always")
      })
      .map(|semi| {
        // The plugin reports at the character before the `;`.
        let before = ctx.source[..semi]
          .char_indices()
          .next_back()
          .map_or(semi, |(i, _)| i);
        Diagnostic::new(NAME, message).span(Span::new(before, 0))
      })
      .collect()
  }
}

/// Whether `decl` is a custom property whose value is a single space
/// (`--x: ;`).
fn is_space_custom_property(decl: DeclRef) -> bool {
  let text = decl.source_text();
  decl.is_custom_property()
    && text
      .split_once(':')
      .is_some_and(|(_, value)| value.strip_suffix(';') == Some(" "))
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  /// The offsets reported for the CSS `source` with `option`.
  fn offsets(source: &str, option: Option<serde_json::Value>) -> Vec<usize> {
    configured(&StylisticDeclarationBlockSemicolonSpaceBefore, option)
      .check_source(&ctx_with_source(source))
      .into_iter()
      .map(|d| d.span.offset)
      .collect()
  }

  #[test]
  fn allows_no_space_before_semicolon() {
    assert!(offsets("a { color: red; }", None).is_empty());
  }

  #[test]
  fn reports_space_before_semicolon_when_never() {
    let d = configured(&StylisticDeclarationBlockSemicolonSpaceBefore, None)
      .check_source(&ctx_with_source("a { color: red ; }"));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected whitespace"));
    assert_eq!(d[0].span.offset, 14);
  }

  #[test]
  fn never_lets_a_custom_property_be_a_space() {
    assert!(offsets("a { --x: ; --y: 1 ; }", None) == [17]);
  }

  #[test]
  fn tests_each_declarations_own_semicolon() {
    let source = "a { color: red ; top: 0; left: 0 }\nb { right: 0; }";
    assert_eq!(offsets(source, None), [14]);
    let always = Some(serde_json::json!("always"));
    assert_eq!(offsets(source, always), [22, 46]);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always-single-line")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always-multi-line")),
      [
        "Invalid option value \"always-multi-line\" for rule \"@stylistic/declaration-block-semicolon-space-before\""
      ]
    );
  }
}
