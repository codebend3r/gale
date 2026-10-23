use gale_css_parser::postcss_tree::{PostcssTree, RuleRef};
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/declaration-block-trailing-semicolon";

declare_rule! {
  /// Require or disallow a trailing semicolon within declaration blocks.
  ///
  /// Equivalent to `@stylistic/declaration-block-trailing-semicolon`.
  pub struct StylisticDeclarationBlockTrailingSemicolon => StylisticDeclarationBlockTrailingSemicolonCheck {
    name: NAME,
    description: "Require or disallow a trailing semicolon within declaration blocks",
    default_severity: Warning,
  }
}

/// `@stylistic/declaration-block-trailing-semicolon` as configured.
pub struct StylisticDeclarationBlockTrailingSemicolonCheck {
  /// The primary option: `"always"` or `"never"`.
  option: &'static str,
  /// `ignore: ["single-declaration"]`: pass over a block holding nothing but
  /// one declaration.
  ignore_single_declaration: bool,
}

impl StylisticDeclarationBlockTrailingSemicolonCheck {
  /// The plugin's options: `"always"` or `"never"`, then `ignore:
  /// ["single-declaration"]`.  Gale reads no option as `"always"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&["always", "never"]);
    let ignore = options.keywords("ignore", &["single-declaration"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always"),
      ignore_single_declaration: ignore.contains(&"single-declaration"),
    })
  }
}

impl Check for StylisticDeclarationBlockTrailingSemicolonCheck {
  /// Flags a style rule whose last declaration's trailing semicolon does
  /// not match the option.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    tree
      .walk_rules()
      .filter_map(|rule| self.check_rule(rule))
      .collect()
  }
}

impl StylisticDeclarationBlockTrailingSemicolonCheck {
  /// The report for `rule`'s last declaration, if it has one.
  fn check_rule(&self, rule: RuleRef) -> Option<Diagnostic> {
    // The plugin checks a declaration only when it is the last node of its
    // block: a rule, at-rule or comment after it ends the block instead.
    let last = rule.children().next_back()?.as_decl()?;
    // Nor is a nested property with a value (`margin: 0 { top: 1px }`).
    if last.has_block() {
      return None;
    }
    // `ignore: ["single-declaration"]` passes over a block whose first
    // node is that declaration too.
    if self.ignore_single_declaration && rule.children().len() == 1 {
      return None;
    }

    // Where PostCSS prints the `;`: after the value, which for a custom
    // property keeps the whitespace after it (`--x: 1 ;`).
    let end = last.span().end().max(last.value_span.end);
    let text = last.source_text();
    let has_semicolon = text.ends_with(';');
    // The plugin reports at the declaration's last character, `;` and the
    // whitespace before it left out.
    let body = text.strip_suffix(';').unwrap_or(text).trim_end();
    let at = Span::new(last.span().offset + body.len().saturating_sub(1), 0);
    match self.option {
      "always" if !has_semicolon => Some(
        Diagnostic::new(NAME, "Expected a trailing semicolon")
          .span(at)
          .fix(Fix::new(
            "Add trailing semicolon",
            vec![Edit::new(Span::new(end, 0), ";")],
          )),
      ),
      "never" if has_semicolon => Some(
        Diagnostic::new(NAME, "Unexpected trailing semicolon")
          .span(at)
          .fix(Fix::new(
            "Remove trailing semicolon",
            vec![Edit::new(Span::new(end - 1, 1), "")],
          )),
      ),
      _ => None,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context, fix, invalid_options};

  /// What the rule reports for `source` in `syntax` with `options`: each
  /// message and its offset.
  fn check(
    source: &str,
    syntax: Syntax,
    options: Option<serde_json::Value>,
  ) -> Vec<(String, usize)> {
    configured(&StylisticDeclarationBlockTrailingSemicolon, options)
      .check_source(&context(source, syntax))
      .into_iter()
      .map(|d| (d.message, d.span.offset))
      .collect()
  }

  #[test]
  fn allows_trailing_semicolon() {
    assert!(check("a { color: red; }", Syntax::Css, None).is_empty());
    let source = ".foo {\n  position: absolute;\n  top: 0;\n}";
    assert!(check(source, Syntax::Css, None).is_empty());
  }

  #[test]
  fn reports_missing_trailing_semicolon_after_the_value() {
    for syntax in [Syntax::Css, Syntax::Scss, Syntax::Less] {
      assert_eq!(
        check("a { color: red\n}", syntax, None),
        [("Expected a trailing semicolon".to_string(), 13)]
      );
    }
    assert_eq!(
      fix(
        NAME,
        serde_json::json!("always"),
        "a { color: red !important }",
        Syntax::Css
      ),
      "a { color: red !important; }"
    );
  }

  #[test]
  fn custom_properties_keep_their_trailing_whitespace() {
    let always = serde_json::json!("always");
    assert_eq!(
      fix(NAME, always.clone(), "a{--x: }", Syntax::Css),
      "a{--x: ;}"
    );
    assert_eq!(
      fix(NAME, always, "a { --y: 1 }", Syntax::Css),
      "a { --y: 1 ;}"
    );
  }

  #[test]
  fn never_reports_the_semicolon() {
    let never = Some(serde_json::json!(["never"]));
    let d = configured(&StylisticDeclarationBlockTrailingSemicolon, never)
      .check_source(&context("a { color: red; }", Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected"));
    assert_eq!((d[0].span.offset, d[0].span.length), (13, 0));
    assert_eq!(
      fix(
        NAME,
        serde_json::json!("never"),
        "a { color: red ; }",
        Syntax::Css
      ),
      "a { color: red  }"
    );
  }

  #[test]
  fn checks_only_a_declaration_that_ends_the_block() {
    let never = Some(serde_json::json!("never"));
    // The nested rule is the last node of `a`: its own last declaration is
    // the one checked.
    let source = "a { color: red; b { top: 0 } }";
    assert!(check(source, Syntax::Scss, never.clone()).is_empty());
    assert_eq!(
      check("a { color: red; b { top: 0; } }", Syntax::Scss, never).len(),
      1
    );
    // Nor is a declaration followed by a comment.
    assert!(check("a { color: red /* c */ }", Syntax::Css, None).is_empty());
  }

  #[test]
  fn checks_scss_nested_property_blocks() {
    // A nested property block (`font: { ... }`) is a block of its own.
    assert_eq!(
      check("a { font: { family: x; size: 1px } }", Syntax::Scss, None),
      [("Expected a trailing semicolon".to_string(), 31)]
    );
    assert!(check("a { margin: 0 { top: 1px; } }", Syntax::Scss, None).is_empty());
  }

  #[test]
  fn ignore_single_declaration_passes_over_a_lone_declaration() {
    let options = serde_json::json!(["always", { "ignore": ["single-declaration"] }]);
    let lint = |source| crate::testing::lint(NAME, options.clone(), source, Syntax::Css);
    assert!(lint("a { color: red }").is_empty());
    assert_eq!(lint("a { /* c */ color: red }").len(), 1);
    assert_eq!(lint("a { color: red; top: 0 }").len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["never", { "ignore": ["single-declaration"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/declaration-block-trailing-semicolon\""
      ]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["always", { "ignore": ["multi-declaration"] }])
      ),
      [
        "Invalid value \"multi-declaration\" for option \"ignore\" of rule \"@stylistic/declaration-block-trailing-semicolon\""
      ]
    );
  }
}
