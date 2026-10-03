use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::pattern::option_matches;
use crate::rule::{Rule, RuleContext};
use crate::standard_syntax::is_standard_syntax_color_function;
use crate::value_parser::{self, NodeKind, ValueNode};

/// Specify modern or legacy notation for color functions.
///
/// Equivalent to Stylelint's `color-function-notation` rule, including its
/// autofix.  Checks `rgb()`, `rgba()`, `hsl()` and `hsla()`.  Primary option:
/// `"modern"` (space-separated channels, `/` before the alpha) or `"legacy"`
/// (comma-separated); only the move to modern notation is fixable.
/// Secondary option `ignore: ["with-var-inside"]` skips calls with a direct
/// `var()` argument.
pub struct ColorFunctionNotation;

/// The color functions that have a legacy, comma-separated notation.
const LEGACY_FUNCTIONS: &[&str] = &["rgb", "rgba", "hsl", "hsla"];

impl Rule for ColorFunctionNotation {
  fn name(&self) -> &'static str {
    "color-function-notation"
  }

  fn description(&self) -> &'static str {
    "Specify modern or legacy notation for color functions"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports color functions in the other notation.  Under `"modern"` the
  /// fix turns the channel commas into spaces, the alpha comma into ` / `,
  /// and drops the `a` of `rgba`/`hsla`.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let modern = match ctx.primary_option_str() {
      Some("modern") => true,
      Some("legacy") => false,
      _ => return Vec::new(),
    };
    let ignore_with_var = option_matches(
      ctx.secondary_options().and_then(|s| s.get("ignore")),
      "with-var-inside",
    );
    let tree = ctx.postcss_tree();
    let mut diags = Vec::new();

    for decl in tree.decls() {
      let Some(value) = ctx.source_slice(decl.value_span.start, decl.value_span.end) else {
        continue;
      };
      if !mentions_legacy_function(value) || (modern && !value.contains(',')) {
        continue;
      }
      let base = decl.value_span.start;
      let parsed = value_parser::parse(value);
      let mut check = |node: &ValueNode<'_>| {
        if node.kind != NodeKind::Function || !is_standard_syntax_color_function(node) {
          return;
        }
        if ignore_with_var
          && node
            .nodes
            .iter()
            .any(|n| n.is_function() && n.value.eq_ignore_ascii_case("var"))
        {
          return;
        }
        let name = node.value.to_ascii_lowercase();
        if !LEGACY_FUNCTIONS.contains(&name.as_str()) || node.nodes.len() < 5 {
          return;
        }
        if is_likely_legacy(&node.nodes) != modern {
          return;
        }
        let span = Span::from_range(base + node.source_index, base + node.source_end_index);
        let mut diag = Diagnostic::new(
          self.name(),
          format!(
            "Expected {} color-function notation",
            if modern { "modern" } else { "legacy" }
          ),
        )
        .severity(self.default_severity())
        .span(span);
        if modern {
          diag = diag.fix(Fix::new(
            "Convert to modern notation",
            modern_edits(node, base),
          ));
        }
        diags.push(diag);
      };
      value_parser::walk(&parsed, &mut |node| {
        check(node);
        true
      });
    }
    diags
  }
}

/// Whether `value` calls a legacy-capable color function (Stylelint tests
/// `/\b(?:hsla|rgba|hsl|rgb)\(/i`).
fn mentions_legacy_function(value: &str) -> bool {
  let lower = value.to_ascii_lowercase();
  lower.match_indices('(').any(|(paren, _)| {
    LEGACY_FUNCTIONS.iter().any(|name| {
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

/// Stylelint's `isLikelyLegacy`: two or three commas and no `/`.
fn is_likely_legacy(nodes: &[ValueNode<'_>]) -> bool {
  let mut commas = 0;
  for node in nodes {
    if node.is_comma() {
      commas += 1;
    } else if node.is_slash() {
      return false;
    }
  }
  commas == 2 || commas == 3
}

/// The edits of Stylelint's modern fixer for the function `node`, whose
/// offsets are relative to `base`: the first two commas become their
/// following whitespace (at least one space), any later comma becomes a `/`
/// with at least one space either side, and `rgba`/`hsla` lose their `a`.
fn modern_edits(node: &ValueNode<'_>, base: usize) -> Vec<Edit> {
  let at_least_one_space = |ws: &str| if ws.is_empty() { " " } else { ws }.to_string();
  let mut edits = Vec::new();
  let mut commas = 0;
  for child in node.nodes.iter().filter(|n| n.is_comma()) {
    let text = if commas < 2 {
      commas += 1;
      at_least_one_space(child.after)
    } else {
      format!(
        "{}/{}",
        at_least_one_space(child.before),
        at_least_one_space(child.after)
      )
    };
    edits.push(Edit::new(
      Span::from_range(base + child.source_index, base + child.source_end_index),
      text,
    ));
  }
  let name = node.value;
  if name.eq_ignore_ascii_case("rgba") || name.eq_ignore_ascii_case("hsla") {
    edits.push(Edit::new(
      Span::new(base + node.source_index, name.len()),
      &name[..name.len() - 1],
    ));
  }
  edits
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, lint};

  const RULE: &str = "color-function-notation";

  #[test]
  fn fixes_to_modern_notation() {
    let modern = || json!(["modern"]);
    assert_eq!(
      fix(
        RULE,
        modern(),
        "a { color: rgba(12, 122, 231, 0.2) }",
        Syntax::Css
      ),
      "a { color: rgb(12 122 231 / 0.2) }"
    );
    assert_eq!(
      fix(
        RULE,
        modern(),
        "a { color: HSLA(120,100%,50%,.5) }",
        Syntax::Css
      ),
      "a { color: HSL(120 100% 50% / .5) }"
    );
    assert_eq!(
      fix(
        RULE,
        modern(),
        "a { color: rgb(0 ,\n  0 , 0) }",
        Syntax::Css
      ),
      "a { color: rgb(0\n  0 0) }"
    );
    assert_eq!(
      fix(
        RULE,
        modern(),
        "a { background: linear-gradient(rgb(0,0,0), hsl(0,0%,0%)) }",
        Syntax::Css
      ),
      "a { background: linear-gradient(rgb(0 0 0), hsl(0 0% 0%)) }"
    );
  }

  #[test]
  fn legacy_is_reported_without_a_fix() {
    let source = "a { color: rgb(0 0 0 / 50%) }";
    assert_eq!(lint(RULE, json!(["legacy"]), source, Syntax::Css).len(), 1);
    assert_eq!(fix(RULE, json!(["legacy"]), source, Syntax::Css), source);
  }

  #[test]
  fn skips_preprocessor_arguments_and_var_when_asked() {
    let modern = || json!(["modern"]);
    assert!(lint(RULE, modern(), "a { color: rgba($a, 0.5) }", Syntax::Scss).is_empty());
    assert!(lint(RULE, modern(), "a { color: rgb(white, .5); }", Syntax::Css).is_empty());
    let ignore_var = json!(["modern", { "ignore": ["with-var-inside"] }]);
    assert!(
      lint(
        RULE,
        ignore_var,
        "a { color: rgba(var(--a), 0.5, 0, 1) }",
        Syntax::Css
      )
      .is_empty()
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = json!(["modern", { "disableFix": true }]);
    let source = "a { color: rgb(0, 0, 0) }";
    assert_eq!(fix(RULE, options.clone(), source, Syntax::Css), source);
    assert_eq!(lint(RULE, options, source, Syntax::Css).len(), 1);
  }
}
