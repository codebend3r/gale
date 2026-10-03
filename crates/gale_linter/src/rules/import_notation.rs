use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::postcss_tree::NodeKind as StatementKind;
use crate::rule::{Rule, RuleContext};
use crate::value_parser::{self, NodeKind};

/// Specify string or URL notation for `@import` rules.
///
/// Equivalent to Stylelint's `import-notation` rule, including its autofix.
/// Primary option `"string"` wants `@import "foo.css"`, `"url"` wants
/// `@import url(foo.css)`.  The fix rewrites the params up to the end of the
/// reported URL or string, keeping media queries and conditions after it.
pub struct ImportNotation;

impl Rule for ImportNotation {
  fn name(&self) -> &'static str {
    "import-notation"
  }

  fn description(&self) -> &'static str {
    "Prefer string notation for @import"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports the first `@import` URL or string in the other notation, with
  /// a fix that rewrites it.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let want_string = match ctx.primary_option_str() {
      Some("string") => true,
      Some("url") => false,
      _ => return Vec::new(),
    };
    let tree = ctx.postcss_tree();
    let mut diags = Vec::new();

    for at_rule in tree
      .nodes
      .iter()
      .filter(|n| n.kind == StatementKind::AtRule)
    {
      let is_import = ctx
        .source_slice(at_rule.name_span.start, at_rule.name_span.end)
        .is_some_and(|name| name.eq_ignore_ascii_case("import"));
      let Some(params) = ctx.source_slice(at_rule.value_span.start, at_rule.value_span.end) else {
        continue;
      };
      if !is_import || has_url_call(params) != want_string {
        continue;
      }
      let base = at_rule.value_span.start;
      for node in value_parser::parse(params) {
        let (message, fixed) = if want_string {
          if !node.is_function() || !node.value.eq_ignore_ascii_case("url") {
            continue;
          }
          let arguments = value_parser::stringify(&node.nodes);
          let quoted = if node.nodes.first().is_some_and(|n| n.is_word()) {
            format!("\"{arguments}\"")
          } else {
            arguments
          };
          let full = node.to_css();
          (format!("Expected \"{full}\" to be \"{quoted}\""), quoted)
        } else {
          if node.kind == NodeKind::Space {
            break;
          }
          if !node.is_word() && node.kind != NodeKind::String {
            continue;
          }
          let path = node.to_css();
          let quoted = match node.quote {
            Some(quote) if node.kind == NodeKind::String => format!("{quote}{}{quote}", node.value),
            _ => format!("\"{}\"", node.value),
          };
          (
            format!("Expected \"{quoted}\" to be \"url({path})\""),
            format!("url({path})"),
          )
        };
        // Stylelint reports from the start of the params to the end of this
        // node, and rewrites the params as the fixed text followed by the
        // rest of PostCSS's cleaned params, which drops comments there.
        let end = node.source_end_index.min(params.len());
        let span = Span::from_range(base, base + end);
        let rest = at_rule.params.get(end..).unwrap_or("");
        diags.push(
          Diagnostic::new(self.name(), message)
            .severity(self.default_severity())
            .span(span)
            .fix(Fix::new(
              format!("Replace with {fixed}"),
              vec![Edit::new(
                Span::from_range(base, base + params.len()),
                format!("{fixed}{rest}"),
              )],
            )),
        );
        break;
      }
    }
    diags
  }
}

/// Whether `params` contains `url(` in any case.
fn has_url_call(params: &str) -> bool {
  params.to_ascii_lowercase().contains("url(")
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, lint};

  const RULE: &str = "import-notation";

  #[test]
  fn fixes_urls_to_strings() {
    let string = || json!(["string"]);
    assert_eq!(
      fix(RULE, string(), "@import url(foo.css) print;", Syntax::Css),
      "@import \"foo.css\" print;"
    );
    assert_eq!(
      fix(RULE, string(), "@import URL( 'foo.css' );", Syntax::Css),
      "@import 'foo.css';"
    );
  }

  #[test]
  fn fixes_strings_to_urls() {
    let url = || json!(["url"]);
    assert_eq!(
      fix(RULE, url(), "@import 'foo.css' print;", Syntax::Css),
      "@import url('foo.css') print;"
    );
    assert_eq!(
      fix(RULE, url(), "@IMPORT \"foo.css\";", Syntax::Css),
      "@IMPORT url(\"foo.css\");"
    );
  }

  #[test]
  fn the_fix_drops_comments_after_the_url_as_stylelint_does() {
    assert_eq!(
      fix(
        RULE,
        json!(["string"]),
        "@import url('a.css') /* a comment */ tv;",
        Syntax::Css
      ),
      "@import 'a.css'  tv;"
    );
  }

  #[test]
  fn reports_the_params_up_to_the_url() {
    let source = "@import url(foo.css) print;";
    let diags = lint(RULE, json!(["string"]), source, Syntax::Css);
    assert_eq!(diags.len(), 1);
    assert_eq!(
      &source[diags[0].span.offset..diags[0].span.end()],
      "url(foo.css)"
    );
    assert_eq!(
      diags[0].message,
      "Expected \"url(foo.css)\" to be \"\"foo.css\"\""
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = json!(["string", { "disableFix": true }]);
    let source = "@import url(foo.css);";
    assert_eq!(fix(RULE, options.clone(), source, Syntax::Css), source);
    assert_eq!(lint(RULE, options, source, Syntax::Css).len(), 1);
  }
}
