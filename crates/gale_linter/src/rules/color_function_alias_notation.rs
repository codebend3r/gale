use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};
use crate::standard_syntax::is_standard_syntax_color_function;
use crate::value_parser::{self, NodeKind, ValueNode};

/// Specify alias notation for color functions.
///
/// Equivalent to Stylelint's `color-function-alias-notation` rule, including
/// its autofix.  Primary option `"with-alpha"` wants `rgba()`/`hsla()`;
/// `"without-alpha"` wants `rgb()`/`hsl()`.  The fix adds or drops the `a`,
/// keeping the name's case.
pub struct ColorFunctionAliasNotation;

/// Names written without the alpha alias.
const WITHOUT_ALPHA: &[&str] = &["rgb", "hsl"];

/// Names written with the alpha alias.
const WITH_ALPHA: &[&str] = &["rgba", "hsla"];

impl Rule for ColorFunctionAliasNotation {
  fn name(&self) -> &'static str {
    "color-function-alias-notation"
  }

  fn description(&self) -> &'static str {
    "Specify modern or legacy notation for color function aliases"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports the name of every color function written in the other alias,
  /// with a fix that renames it.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let with_alpha = match ctx.primary_option_str() {
      Some("with-alpha") => true,
      Some("without-alpha") => false,
      _ => return Vec::new(),
    };
    // The names to report: the ones in the other notation.
    let targets = if with_alpha {
      WITHOUT_ALPHA
    } else {
      WITH_ALPHA
    };
    let tree = ctx.postcss_tree();
    let mut diags = Vec::new();

    for decl in tree.decls() {
      let Some(value) = ctx.source_slice(decl.value_span.start, decl.value_span.end) else {
        continue;
      };
      if !mentions_function(value, targets) {
        continue;
      }
      let parsed = value_parser::parse(value);
      let mut check = |node: &ValueNode<'_>| {
        if node.kind != NodeKind::Function || !is_standard_syntax_color_function(node) {
          return;
        }
        let name = node.value;
        if !targets.iter().any(|t| t.eq_ignore_ascii_case(name)) {
          return;
        }
        let fixed = if with_alpha {
          format!("{name}a")
        } else {
          name[..name.len() - 1].to_string()
        };
        let span = Span::new(decl.value_span.start + node.source_index, name.len());
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Expected \"{name}\" to be \"{fixed}\""),
          )
          .severity(self.default_severity())
          .span(span)
          .fix(Fix::new(
            format!("Rename \"{name}\" to \"{fixed}\""),
            vec![Edit::new(span, fixed.clone())],
          )),
        );
      };
      value_parser::walk(&parsed, &mut |node| {
        check(node);
        true
      });
    }
    diags
  }
}

/// Whether `value` calls one of `names` (Stylelint tests
/// `/\b(?:name|...)\(/i`).
fn mentions_function(value: &str, names: &[&str]) -> bool {
  let lower = value.to_ascii_lowercase();
  lower.match_indices('(').any(|(paren, _)| {
    names.iter().any(|name| {
      lower[..paren].ends_with(name) && {
        let start = paren - name.len();
        start == 0 || {
          let prev = lower.as_bytes()[start - 1];
          !(prev.is_ascii_alphanumeric() || prev == b'_')
        }
      }
    })
  })
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, lint};

  const RULE: &str = "color-function-alias-notation";

  #[test]
  fn drops_the_alpha_alias() {
    assert_eq!(
      fix(
        RULE,
        json!(["without-alpha"]),
        "a { color: RGBA(0 0 0 / 0.5), hsla(0, 0%, 0%) }",
        Syntax::Css
      ),
      "a { color: RGB(0 0 0 / 0.5), hsl(0, 0%, 0%) }"
    );
  }

  #[test]
  fn adds_the_alpha_alias() {
    assert_eq!(
      fix(
        RULE,
        json!(["with-alpha"]),
        "a { color: Rgb(0 0 0) }",
        Syntax::Css
      ),
      "a { color: Rgba(0 0 0) }"
    );
  }

  #[test]
  fn skips_preprocessor_arguments() {
    assert!(
      lint(
        RULE,
        json!(["without-alpha"]),
        "a { color: rgba($c, 0.5) }",
        Syntax::Scss
      )
      .is_empty()
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = json!(["without-alpha", { "disableFix": true }]);
    let source = "a { color: rgba(0 0 0) }";
    assert_eq!(fix(RULE, options.clone(), source, Syntax::Css), source);
    assert_eq!(lint(RULE, options, source, Syntax::Css).len(), 1);
  }
}
