use gale_css_parser::postcss_tree::{DeclRef, PostcssTree};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/declaration-block-semicolon-newline-before";

declare_rule! {
  /// Require a newline or disallow whitespace before the semicolons of declaration blocks.
  ///
  /// Equivalent to `@stylistic/declaration-block-semicolon-newline-before`.
  pub struct StylisticDeclarationBlockSemicolonNewlineBefore => StylisticDeclarationBlockSemicolonNewlineBeforeCheck {
    name: NAME,
    description: "Require a newline or disallow whitespace before the semicolons of declaration blocks",
    default_severity: Warning,
  }
}

/// `@stylistic/declaration-block-semicolon-newline-before` as configured.
pub struct StylisticDeclarationBlockSemicolonNewlineBeforeCheck {
  /// The primary option: `"always"`, `"always-multi-line"`,
  /// `"never-multi-line"` or gale's own `"never"`.
  option: &'static str,
}

impl StylisticDeclarationBlockSemicolonNewlineBeforeCheck {
  /// The plugin's options: `"always"`, `"always-multi-line"` or
  /// `"never-multi-line"`; gale applies only `"always"` so far.  Gale also
  /// accepts `"never"`, which the plugin does not: it disallows whitespace before
  /// the semicolon, and is what gale reads no option as.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option =
      options.primary_one_of(&["always", "always-multi-line", "never-multi-line", "never"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("never"),
    })
  }
}

impl Check for StylisticDeclarationBlockSemicolonNewlineBeforeCheck {
  /// Flags a declaration semicolon whose preceding newline does not match the option.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let message = match self.option {
      "always" => "Expected newline before \";\"",
      "never" => "Unexpected whitespace before \";\"",
      _ => return vec![],
    };
    let source = ctx.source.as_bytes();
    tree
      .walk_decls()
      .filter(|decl| decl.parent_is_rule())
      .filter_map(DeclRef::semicolon)
      .filter(|&semi| {
        let has_newline_before = matches!(source[semi - 1], b'\n' | b'\r');
        has_newline_before != (self.option == "always")
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

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  /// The offsets reported for the CSS `source` with `option`.
  fn offsets(source: &str, option: Option<serde_json::Value>) -> Vec<usize> {
    configured(&StylisticDeclarationBlockSemicolonNewlineBefore, option)
      .check_source(&ctx_with_source(source))
      .into_iter()
      .map(|d| d.span.offset)
      .collect()
  }

  #[test]
  fn allows_no_newline_before_semicolon() {
    assert!(offsets("a { color: red; }", None).is_empty());
  }

  #[test]
  fn reports_newline_before_semicolon_when_never() {
    let d = configured(
      &StylisticDeclarationBlockSemicolonNewlineBefore,
      serde_json::json!("never"),
    )
    .check_source(&ctx_with_source("a { color: red\n; }"));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected"));
    assert_eq!(d[0].span.offset, 14);
  }

  #[test]
  fn tests_each_declarations_own_semicolon() {
    let source = "a { color: red\n; top: 0; left: 0 }\nb { right: 0; }";
    assert_eq!(offsets(source, None), [14]);
    let always = Some(serde_json::json!("always"));
    assert_eq!(offsets(source, always), [22, 46]);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always-multi-line")).is_empty());
    // `"never"` is gale's own value; the plugin rejects it.
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/declaration-block-semicolon-newline-before\""
      ]
    );
  }
}
