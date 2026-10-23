use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "declaration-no-important";

declare_rule! {
  /// Reports use of `!important` in declarations.
  ///
  /// Equivalent to Stylelint's `declaration-no-important` rule.
  pub struct DeclarationNoImportant => DeclarationNoImportantCheck {
    name: NAME,
    description: "Disallow !important within declarations",
    default_severity: Warning,
  }
}

/// `declaration-no-important`, which takes no options.
pub struct DeclarationNoImportantCheck;

impl DeclarationNoImportantCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for DeclarationNoImportantCheck {
  /// Flags `!important` declarations in style rules, locating the token in
  /// the source so the fix removes it along with the whitespace in front
  /// of it.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    tree
      .walk_decls()
      .filter(|decl| decl.important() && decl.parent_is_rule())
      .map(|decl| {
        let message = format!("Unexpected !important in declaration \"{}\"", decl.prop());
        let decl_start = decl.span().offset;
        let slice = decl.source_text();
        // Point the diagnostic at the `!important` text when it is written
        // that way, so that `disable-line` comments on its line (which may
        // differ from the declaration's first line) suppress it.
        let Some(pos) = find_important(slice) else {
          return Diagnostic::new(NAME, message).span(decl.span());
        };
        let imp_len = slice[pos..]
          .find(';')
          .unwrap_or(slice.len() - pos)
          .min("!important".len());
        // Remove the whitespace before the `!` too.
        let leading_ws = slice[..pos]
          .bytes()
          .rev()
          .take_while(|b| *b == b' ' || *b == b'\t')
          .count();
        let fix = Fix::new(
          "Remove !important",
          vec![Edit::new(
            Span::new(decl_start + pos - leading_ws, imp_len + leading_ws),
            "",
          )],
        );
        Diagnostic::new(NAME, message)
          .span(Span::new(decl_start + pos, "!important".len()))
          .fix(fix)
      })
      .collect()
  }
}

/// Find the byte offset of `!important` (case-insensitive) in a string slice.
fn find_important(s: &str) -> Option<usize> {
  let lower = s.to_ascii_lowercase();
  lower.find("!important")
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, ctx_with_source, fix};

  #[test]
  fn reports_important() {
    let src = "a { color: red !important; }";
    let d = configured(&DeclarationNoImportant, None).check_source(&ctx_with_source(src));
    assert_eq!(d.len(), 1);
    assert_eq!(
      d[0].message,
      "Unexpected !important in declaration \"color\""
    );
    assert_eq!((d[0].span.offset, d[0].span.length), (15, 10));
    assert!(d[0].fix.is_some());
  }

  #[test]
  fn allows_no_important() {
    let src = "a { color: red; }";
    let d = configured(&DeclarationNoImportant, None).check_source(&ctx_with_source(src));
    assert!(d.is_empty());
  }

  #[test]
  fn checks_style_rule_declarations_only() {
    let src = "@font-face { font-family: x !important; }";
    let d = configured(&DeclarationNoImportant, None).check_source(&ctx_with_source(src));
    assert!(d.is_empty());
  }

  #[test]
  fn fix_removes_the_flag_and_the_space_before_it() {
    let options = serde_json::json!(true);
    for syntax in [Syntax::Css, Syntax::Scss] {
      assert_eq!(
        fix(
          "declaration-no-important",
          options.clone(),
          "a { color: red !IMPORTANT; top: 0 !important }",
          syntax
        ),
        "a { color: red; top: 0 }"
      );
    }
  }

  #[test]
  fn spaced_flag_reports_the_declaration_without_a_fix() {
    let src = "a { color: red ! important; }";
    let d = configured(&DeclarationNoImportant, None).check_source(&ctx_with_source(src));
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].span.offset, d[0].span.length), (4, 23));
    assert!(d[0].fix.is_none());
  }
}
