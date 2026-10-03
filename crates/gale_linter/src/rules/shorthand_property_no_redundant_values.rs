use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::autoprefixable::vendor_prefix_len;
use crate::rule::{Rule, RuleContext};
use crate::source_text;
use crate::value_parser::{self, NodeKind, ValueNode};

/// Reports shorthand property values that contain redundant parts
/// (e.g. `margin: 1px 1px 1px 1px` → `margin: 1px`).
///
/// Equivalent to Stylelint's `shorthand-property-no-redundant-values` rule.
/// The fix replaces the value as written with its shortest form and leaves
/// `!important` alone.
pub struct ShorthandPropertyNoRedundantValues;

/// Returns `true` if the value is "standard CSS syntax" — i.e., it does not
/// contain SCSS/Less constructs that would make comparison unreliable.
fn is_standard_syntax_value(value: &str) -> bool {
  if value.contains('$') || value.contains("#{") || value.contains("@{") {
    return false;
  }
  // Less variables: @variable
  if value.contains('@') {
    // Check if any @ is followed by an alpha char (Less variable).
    // But allow @media etc. won't appear in values normally.
    let bytes = value.as_bytes();
    for i in 0..bytes.len().saturating_sub(1) {
      if bytes[i] == b'@' && bytes[i + 1].is_ascii_alphabetic() {
        return false;
      }
    }
  }
  // SCSS module function call: `namespace.function(`
  let bytes = value.as_bytes();
  for i in 1..bytes.len().saturating_sub(1) {
    if bytes[i] == b'.'
      && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'-' || bytes[i - 1] == b'_')
      && (bytes[i + 1].is_ascii_alphabetic() || bytes[i + 1] == b'-' || bytes[i + 1] == b'_')
    {
      return false;
    }
  }
  true
}

/// Stylelint's `SUPPORTED_SHORTHANDS`.
const SHORTHAND_PROPERTIES: &[&str] = &[
  "margin",
  "margin-block",
  "margin-inline",
  "padding",
  "padding-block",
  "padding-inline",
  "border-color",
  "border-style",
  "border-width",
  "border-radius",
  "border-block-color",
  "border-block-style",
  "border-block-width",
  "border-inline-color",
  "border-inline-style",
  "border-inline-width",
  "gap",
  "grid-gap",
  "overflow",
  "overscroll-behavior",
  "scroll-margin",
  "scroll-margin-block",
  "scroll-margin-inline",
  "scroll-padding",
  "scroll-padding-block",
  "scroll-padding-inline",
  "inset",
  "inset-block",
  "inset-inline",
];

/// Stylelint's `FOUR_DIRECTIONAL_SHORTHAND_PROPERTIES`, the ones the
/// `four-into-three-edge-values` exception applies to.
const FOUR_DIRECTIONAL_PROPERTIES: &[&str] = &[
  "margin",
  "padding",
  "border-color",
  "border-style",
  "border-width",
  "scroll-margin",
  "scroll-padding",
  "inset",
];

/// Whether the `ignore: ["four-into-three-edge-values"]` option is set.
fn ignores_four_into_three(ctx: &RuleContext) -> bool {
  ctx
    .secondary_options()
    .and_then(|v| v.get("ignore"))
    .and_then(|v| v.as_array())
    .is_some_and(|items| {
      items
        .iter()
        .any(|item| item.as_str() == Some("four-into-three-edge-values"))
    })
}

impl Rule for ShorthandPropertyNoRedundantValues {
  fn name(&self) -> &'static str {
    "shorthand-property-no-redundant-values"
  }

  fn description(&self) -> &'static str {
    "Disallow redundant values within shorthand properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags shorthand values that repeat parts a shorter form would imply, and
  /// offers the shortened value as a fix.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let decls: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };
    let ignore_four_into_three = ignores_four_into_three(ctx);
    let mut diags = Vec::new();
    for decl in decls {
      let Some((prop, _)) = source_text::declaration_property(ctx.source, decl) else {
        continue;
      };
      // Custom properties, Sass and Less variables are not shorthands.
      if prop.starts_with("--") || prop.starts_with('$') || prop.starts_with('@') {
        continue;
      }
      let Some((value, value_start)) = source_text::declaration_value(ctx.source, decl) else {
        continue;
      };
      if !value.contains(char::is_whitespace) || !is_standard_syntax_value(value) {
        continue;
      }
      let lower = prop.to_ascii_lowercase();
      let normalized = &lower[vendor_prefix_len(&lower)..];
      if !SHORTHAND_PROPERTIES.contains(&normalized) {
        continue;
      }

      let nodes = value_parser::parse(value);
      let shortest = if normalized == "border-radius"
        && let Some(radius) = split_border_radius(&nodes)
      {
        shortest_border_radius(value, &radius)
      } else {
        shortest_values(&nodes, normalized, ignore_four_into_three)
      };
      let Some(shortest) = shortest else {
        continue;
      };

      let span = Span::new(value_start, value.len());
      diags.push(
        Diagnostic::new(
          self.name(),
          format!("Expected \"{value}\" to be \"{shortest}\""),
        )
        .severity(self.default_severity())
        .span(span)
        .fix(Fix::new(
          format!("Shorten to \"{shortest}\""),
          vec![Edit::new(span, &shortest)],
        )),
      );
    }
    diags
  }
}

/// Whether a node is a `var()` call.
fn is_var(node: &ValueNode) -> bool {
  node.is_function_named("var")
}

/// The shortest form of a shorthand value without a `border-radius` slash,
/// or `None` when it is already as short as it gets (or holds a divider or
/// `var()`, which Stylelint leaves alone).
fn shortest_values(
  nodes: &[ValueNode],
  property: &str,
  ignore_four_into_three: bool,
) -> Option<String> {
  let mut values = Vec::new();
  for node in nodes {
    if node.kind == NodeKind::Div || is_var(node) {
      return None;
    }
    if matches!(node.kind, NodeKind::Word | NodeKind::Function) {
      values.push(node.to_css());
    }
  }
  if values.len() <= 1 || values.len() > 4 {
    return None;
  }
  let shortest = condense(&values);
  if ignore_four_into_three
    && FOUR_DIRECTIONAL_PROPERTIES.contains(&property)
    && values.len() == 4
    && shortest.len() == 3
  {
    return None;
  }
  let shortest = shortest.join(" ");
  (!shortest.eq_ignore_ascii_case(&values.join(" "))).then_some(shortest)
}

/// A `border-radius` value split at its slash.
struct BorderRadius {
  horizontal: Vec<String>,
  vertical: Vec<String>,
  horizontal_has_var: bool,
  vertical_has_var: bool,
}

/// Split a `border-radius` value into its horizontal and vertical radii, as
/// Stylelint's `splitBorderRadius` does.  `None` when there is no slash, or
/// when another divider makes it unsafe to lint.
fn split_border_radius(nodes: &[ValueNode]) -> Option<BorderRadius> {
  let mut radius = BorderRadius {
    horizontal: Vec::new(),
    vertical: Vec::new(),
    horizontal_has_var: false,
    vertical_has_var: false,
  };
  let mut saw_slash = false;
  for node in nodes {
    if node.kind == NodeKind::Div {
      if node.value != "/" {
        return None;
      }
      saw_slash = true;
      continue;
    }
    if is_var(node) {
      if saw_slash {
        radius.vertical_has_var = true;
      } else {
        radius.horizontal_has_var = true;
      }
    }
    if matches!(node.kind, NodeKind::Word | NodeKind::Function) {
      let side = if saw_slash {
        &mut radius.vertical
      } else {
        &mut radius.horizontal
      };
      side.push(node.to_css());
    }
  }
  saw_slash.then_some(radius)
}

/// The shortest form of a `border-radius` value with a slash, condensing
/// each side that holds no `var()`, or `None` when the value already is
/// that (ignoring case and runs of whitespace).
fn shortest_border_radius(value: &str, radius: &BorderRadius) -> Option<String> {
  let side = |values: &[String], has_var: bool| {
    if has_var {
      values.join(" ")
    } else {
      condense(values).join(" ")
    }
  };
  let shortest = format!(
    "{} / {}",
    side(&radius.horizontal, radius.horizontal_has_var),
    side(&radius.vertical, radius.vertical_has_var)
  )
  .trim()
  .to_string();
  let normalized = value
    .trim()
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ")
    .to_ascii_lowercase();
  (normalized != shortest.to_ascii_lowercase()).then_some(shortest)
}

/// Stylelint's `canCondense`: the fewest of `values` (top, right, bottom,
/// left; missing ones empty) that mean the same, compared ignoring case.
fn condense(values: &[String]) -> Vec<String> {
  let get = |i: usize| values.get(i).map_or("", String::as_str);
  let (top, right, bottom, left) = (get(0), get(1), get(2), get(3));
  let (t, r, b, l) = (
    top.to_ascii_lowercase(),
    right.to_ascii_lowercase(),
    bottom.to_ascii_lowercase(),
    left.to_ascii_lowercase(),
  );
  let kept: &[&str] =
    if t == r && ((t == b && (b == l || l.is_empty())) || (b.is_empty() && l.is_empty())) {
      &[top]
    } else if (t == b && r == l) || (t == b && l.is_empty() && t != r) {
      &[top, right]
    } else if r == l {
      &[top, right, bottom]
    } else {
      &[top, right, bottom, left]
    };
  kept
    .iter()
    .filter(|v| !v.is_empty())
    .map(|v| v.to_string())
    .collect()
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;

  use crate::testing::{fix, lint};

  const RULE: &str = "shorthand-property-no-redundant-values";

  #[test]
  fn shortens_and_keeps_important() {
    let on = serde_json::json!(true);
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { margin: 1px 1px 1px 1px; }",
        Syntax::Css
      ),
      "a { margin: 1px; }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { margin: 1px 1px !important; }",
        Syntax::Css
      ),
      "a { margin: 1px !important; }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { padding: 1Px 2px 1pX 2px; }",
        Syntax::Css
      ),
      "a { padding: 1Px 2px; }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { margin: calc(1px + 1px) calc(1px + 1px); }",
        Syntax::Css
      ),
      "a { margin: calc(1px + 1px); }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { -webkit-border-radius: 1px 1px 1px 1px; }",
        Syntax::Css
      ),
      "a { -webkit-border-radius: 1px; }"
    );
    let warnings = lint(RULE, on, "a { margin-inline: 1px 1px; }", Syntax::Css);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].message, "Expected \"1px 1px\" to be \"1px\"");
  }

  #[test]
  fn condenses_only_the_border_radius_side_without_var() {
    let on = serde_json::json!(true);
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { border-radius: 1px 1px / var(--foo) var(--foo); }",
        Syntax::Css
      ),
      "a { border-radius: 1px / var(--foo) var(--foo); }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { border-radius: 1px 1px 1px 1px / 2px 2px 2px 2px; }",
        Syntax::Css
      ),
      "a { border-radius: 1px / 2px; }"
    );
    assert!(lint(RULE, on, "a { border-radius: 1px / 2px; }", Syntax::Css).is_empty());
  }

  #[test]
  fn leaves_var_dividers_and_other_properties_alone() {
    let on = serde_json::json!(true);
    for css in [
      "a { margin: var(--margin) var(--margin); }",
      "a { margin: 1px 2px 3px 4px; }",
      "a { border: 5px solid red; }",
      "a { margin: 1px, 1px; }",
      "a { margin: 1px; }",
    ] {
      assert!(lint(RULE, on.clone(), css, Syntax::Css).is_empty(), "{css}");
    }
  }

  #[test]
  fn four_into_three_exception_covers_edge_properties_only() {
    let options = serde_json::json!([true, { "ignore": ["four-into-three-edge-values"] }]);
    assert!(
      lint(
        RULE,
        options.clone(),
        "a { margin: 1px 2px 3px 2px; }",
        Syntax::Css
      )
      .is_empty()
    );
    assert_eq!(
      fix(
        RULE,
        options,
        "a { border-radius: 1px 2px 3px 2px; }",
        Syntax::Css
      ),
      "a { border-radius: 1px 2px 3px; }"
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!([true, { "disableFix": true }]);
    assert_eq!(
      lint(RULE, options.clone(), "a { gap: 1rem 1rem; }", Syntax::Css).len(),
      1
    );
    assert_eq!(
      fix(RULE, options, "a { gap: 1rem 1rem; }", Syntax::Css),
      "a { gap: 1rem 1rem; }"
    );
  }
}
