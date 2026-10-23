use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "gale/scss-no-import";

declare_rule! {
  /// Disallow the Sass `@import` rule, which Dart Sass deprecated in 1.80.0
  /// and removes in 3.0.0.
  ///
  /// Gale's own rule; Stylelint and stylelint-scss have no equivalent.
  ///
  /// Only a Sass import is reported.  The imports Sass passes through as plain
  /// CSS, and keeps after 3.0.0, are left alone: a URL that ends in `.css`,
  /// starts with `http://`, `https://` or `//`, is written as `url()` or
  /// contains interpolation, and an import with media queries or any other
  /// condition after the URL.
  ///
  /// ```scss
  /// @import "variables";      // reported: use @use or @forward
  /// @import "theme.css";      // plain CSS, allowed
  /// @import "print" print;    // plain CSS, allowed
  /// ```
  pub struct GaleScssNoImport => GaleScssNoImportCheck {
    name: NAME,
    description: "Disallow Sass @import, which Dart Sass 3 removes",
    default_severity: Warning,
  }
}

/// `gale/scss-no-import`, which takes no options.
pub struct GaleScssNoImportCheck;

impl GaleScssNoImportCheck {
  /// Gale's own rule takes nothing but `true`, like a Stylelint rule
  /// that validates `{ actual: primary }` with nothing possible.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

/// Splits `params` on the commas between imports, ignoring commas inside
/// strings and parentheses.
fn split_imports(params: &str) -> Vec<&str> {
  let mut parts = Vec::new();
  let mut depth = 0usize;
  let mut quote: Option<char> = None;
  let mut start = 0;
  let mut escaped = false;
  for (i, c) in params.char_indices() {
    if escaped {
      escaped = false;
      continue;
    }
    match (quote, c) {
      (_, '\\') => escaped = true,
      (Some(q), c) if c == q => quote = None,
      (Some(_), _) => {}
      (None, '"' | '\'') => quote = Some(c),
      (None, '(') => depth += 1,
      (None, ')') => depth = depth.saturating_sub(1),
      (None, ',') if depth == 0 => {
        parts.push(&params[start..i]);
        start = i + 1;
      }
      _ => {}
    }
  }
  parts.push(&params[start..]);
  parts
}

/// Whether one import in an `@import` list is one Sass passes through as
/// plain CSS rather than loading itself.
fn is_plain_css_import(import: &str) -> bool {
  let import = import.trim();
  if import.is_empty() || import.contains("#{") {
    return true;
  }
  if import.len() >= 4 && import[..4].eq_ignore_ascii_case("url(") {
    return true;
  }

  let (url, rest) = match import.chars().next() {
    Some(q @ ('"' | '\'')) => match import[1..].find(q) {
      Some(end) => (&import[1..1 + end], &import[2 + end..]),
      None => (&import[1..], ""),
    },
    _ => match import.find(char::is_whitespace) {
      Some(end) => (&import[..end], &import[end..]),
      None => (import, ""),
    },
  };

  !rest.trim().is_empty()
    || url.to_ascii_lowercase().ends_with(".css")
    || url.starts_with("http://")
    || url.starts_with("https://")
    || url.starts_with("//")
}

impl Check for GaleScssNoImportCheck {
  /// Reports an `@import` that loads at least one Sass file.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    tree
      .walk_at_rules()
      .filter(|at| at.name().eq_ignore_ascii_case("import"))
      .filter(|at| {
        !split_imports(&at.raw_params())
          .into_iter()
          .all(is_plain_css_import)
      })
      .map(|at| {
        Diagnostic::new(NAME, "Expected @use or @forward instead of @import").span(at.span())
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, context};

  /// The reports for `@import <params>;` in `syntax`.
  fn check(params: &str, syntax: Syntax) -> Vec<Diagnostic> {
    let source = format!("@import {params};");
    configured(&GaleScssNoImport, None).check_source(&context(&source, syntax))
  }

  fn reports(params: &str) -> bool {
    !check(params, Syntax::Scss).is_empty()
  }

  #[test]
  fn reports_a_sass_import() {
    let diags = check("\"variables\"", Syntax::Scss);
    assert_eq!(diags.len(), 1);
    assert_eq!(
      diags[0].message,
      "Expected @use or @forward instead of @import"
    );
    assert_eq!((diags[0].span.offset, diags[0].span.length), (0, 20));
  }

  #[test]
  fn reports_a_list_once_when_any_import_is_sass() {
    assert!(reports("\"mixins\", \"functions\""));
    assert!(reports("\"theme.css\", \"mixins\""));
    assert_eq!(check("\"a\", \"b\"", Syntax::Scss).len(), 1);
  }

  #[test]
  fn reports_partials_and_sass_extensions() {
    assert!(reports("'_partial'"));
    assert!(reports("\"theme.scss\""));
    assert!(reports("foo"));
  }

  #[test]
  fn allows_plain_css_imports() {
    for params in [
      "\"theme.css\"",
      "\"THEME.CSS\"",
      "url(\"fonts.css\")",
      "url(fonts)",
      "\"https://example.com/reset\"",
      "\"http://example.com/reset\"",
      "\"//cdn.example.com/reset\"",
      "\"print\" print",
      "\"layout\" supports(display: grid)",
      "\"layout\" layer(base)",
      "\"theme-#{$name}\"",
      "\"a.css\", url(b, c)",
    ] {
      assert!(!reports(params), "{params} should be plain CSS");
    }
  }

  #[test]
  fn skips_css_files() {
    assert!(check("\"variables\"", Syntax::Css).is_empty());
  }

  #[test]
  fn ignores_other_at_rules() {
    let source = "@use \"variables\";";
    let rule = configured(&GaleScssNoImport, None);
    assert!(rule.check_source(&context(source, Syntax::Scss)).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(invalid_options("gale/scss-no-import", serde_json::json!(true)).is_empty());
    // The rule takes nothing but `true`, and ignores secondary options.
    assert!(
      invalid_options(
        "gale/scss-no-import",
        serde_json::json!([true, { "foo": 1 }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options("gale/scss-no-import", serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"gale/scss-no-import\""]
    );
  }
}
