use gale_css_parser::{CssNode, Declaration, StyleRule};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow declarations that have no effect because another declaration in
/// the same block switches them off.
///
/// Gale's own rule; Stylelint has no equivalent.
///
/// ```css
/// a {
///   display: block;
///   justify-content: center; /* no effect: not a flex or grid container */
/// }
/// ```
///
/// Only what the block itself proves is reported, so there are no false
/// positives from the cascade: a `justify-content` with no `display` in its
/// block may sit on a flex container another rule made, and is left alone.
/// So are values that come from variables, blocks that include a mixin or
/// extend a selector (which may set `display` or `position`), and
/// declarations that set a property to its initial value or a CSS-wide
/// keyword, which are usually deliberate resets.
///
/// | Declaration | No effect with |
/// |---|---|
/// | Flex and grid container properties | A `display` that is neither flex nor grid |
/// | `gap`, `row-gap`, `column-gap` | The same, outside a multi-column container |
/// | `top`, `right`, `bottom`, `left`, `inset*` | `position: static` |
/// | `float` | `position: absolute` or `fixed` |
/// | `vertical-align` | A block-level `display` |
/// | `table-layout` | A `display` that is not a table |
pub struct GaleNoIneffectiveDeclarations;

/// Container properties that only apply to flex and grid containers.
/// `justify-content` also applies to multi-column containers.
const CONTAINER_PROPERTIES: &[&str] = &[
  "justify-content",
  "align-items",
  "flex-direction",
  "flex-wrap",
  "flex-flow",
  "grid-template",
  "grid-template-columns",
  "grid-template-rows",
  "grid-template-areas",
  "grid-auto-flow",
  "grid-auto-columns",
  "grid-auto-rows",
];

/// Gap properties, which apply to flex, grid and multi-column containers.
const GAP_PROPERTIES: &[&str] = &["gap", "row-gap", "column-gap"];

/// Properties that make a box a multi-column container.
const MULTICOL_PROPERTIES: &[&str] = &["columns", "column-count", "column-width"];

/// Offsets, which a statically positioned box ignores.
const OFFSET_PROPERTIES: &[&str] = &[
  "top",
  "right",
  "bottom",
  "left",
  "inset",
  "inset-block",
  "inset-block-start",
  "inset-block-end",
  "inset-inline",
  "inset-inline-start",
  "inset-inline-end",
];

/// Every `display` keyword the rule understands.  A value with any other
/// word in it is left alone.
const DISPLAY_KEYWORDS: &[&str] = &[
  "block",
  "inline",
  "inline-block",
  "flow",
  "flow-root",
  "list-item",
  "flex",
  "inline-flex",
  "grid",
  "inline-grid",
  "table",
  "inline-table",
  "table-row",
  "table-cell",
  "table-row-group",
  "table-header-group",
  "table-footer-group",
  "table-column",
  "table-column-group",
  "table-caption",
  "ruby",
  "ruby-base",
  "ruby-text",
  "ruby-base-container",
  "ruby-text-container",
];

/// Values that never count as "no effect": the CSS-wide keywords.
const CSS_WIDE_KEYWORDS: &[&str] = &["inherit", "initial", "unset", "revert", "revert-layer"];

/// The initial value of each property the rule reports, lowercased.  Setting
/// a property to it is a reset, which is left alone.
fn is_initial_value(property: &str, value: &str) -> bool {
  let initial: &[&str] = match property {
    "justify-content" | "align-items" | "gap" | "row-gap" | "column-gap" => &["normal"],
    "flex-direction" => &["row"],
    "flex-wrap" => &["nowrap"],
    "flex-flow" => &["row", "nowrap", "row nowrap"],
    "grid-auto-flow" => &["row"],
    "grid-auto-columns" | "grid-auto-rows" | "table-layout" => &["auto"],
    "float" => &["none"],
    "vertical-align" => &["baseline"],
    p if p.starts_with("grid-template") => &["none"],
    p if OFFSET_PROPERTIES.contains(&p) => &["auto"],
    _ => &[],
  };
  initial.contains(&value)
}

/// Whether `value` depends on something the block cannot see.
fn is_dynamic(value: &str) -> bool {
  ["var(", "env(", "attr(", "$", "@", "#{", "~\""]
    .iter()
    .any(|marker| value.contains(marker))
}

/// The winning declaration of `property` in `declarations`, by the cascade
/// inside one block: the last one, unless an earlier one is `!important`
/// and it is not.
fn winning<'a>(declarations: &'a [Declaration], property: &str) -> Option<&'a Declaration> {
  let mut winner: Option<&Declaration> = None;
  for decl in declarations {
    if decl.property.eq_ignore_ascii_case(property)
      && (decl.important || !winner.is_some_and(|w| w.important))
    {
      winner = Some(decl);
    }
  }
  winner
}

/// What the block's own `display` and `position` say about the box.
struct Layout<'a> {
  /// The winning `display`, as written, with its lowercased keywords; only
  /// set when every keyword is one the rule understands.
  display: Option<(&'a str, Vec<String>)>,
  /// The winning `position`, as written and lowercased.
  position: Option<(&'a str, String)>,
  /// Whether the block makes the box a multi-column container.
  multicol: bool,
}

impl<'a> Layout<'a> {
  /// Reads the box from a block's declarations.
  fn read(declarations: &'a [Declaration]) -> Self {
    let display = winning(declarations, "display")
      .map(|decl| decl.value.trim())
      .filter(|value| !is_dynamic(value))
      .and_then(|value| {
        let keywords: Vec<String> = value
          .split_whitespace()
          .map(str::to_ascii_lowercase)
          .collect();
        let known = !keywords.is_empty()
          && keywords
            .iter()
            .all(|word| DISPLAY_KEYWORDS.contains(&word.as_str()));
        known.then_some((value, keywords))
      });
    let position = winning(declarations, "position")
      .map(|decl| decl.value.trim())
      .filter(|value| !is_dynamic(value))
      .map(|value| (value, value.to_ascii_lowercase()));
    let multicol = declarations.iter().any(|decl| {
      MULTICOL_PROPERTIES
        .iter()
        .any(|p| decl.property.eq_ignore_ascii_case(p))
    });
    Self {
      display,
      position,
      multicol,
    }
  }

  /// Whether the display makes a flex or grid container.
  fn is_flex_or_grid(keywords: &[String]) -> bool {
    keywords.iter().any(|word| {
      matches!(
        word.as_str(),
        "flex" | "inline-flex" | "grid" | "inline-grid"
      )
    })
  }

  /// Whether the display makes a block-level box: a single block-level
  /// keyword, or a two-keyword value whose outer display is `block`.
  fn is_block_level(keywords: &[String]) -> bool {
    match keywords {
      [one] => matches!(
        one.as_str(),
        "block" | "flex" | "grid" | "flow-root" | "list-item" | "table"
      ),
      [outer, ..] => outer == "block",
      [] => false,
    }
  }

  /// The declaration that switches `property` off, as `"name: value"`.
  fn switched_off_by(&self, property: &str) -> Option<String> {
    let display = |(value, _): &(&str, Vec<String>)| format!("display: {value}");
    let position = |(value, _): &(&str, String)| format!("position: {value}");

    if CONTAINER_PROPERTIES.contains(&property) || GAP_PROPERTIES.contains(&property) {
      if self.multicol {
        return None;
      }
      return self
        .display
        .as_ref()
        .filter(|(_, keywords)| !Self::is_flex_or_grid(keywords))
        .map(display);
    }
    if OFFSET_PROPERTIES.contains(&property) {
      return self
        .position
        .as_ref()
        .filter(|(_, value)| value == "static")
        .map(position);
    }
    match property {
      "float" => self
        .position
        .as_ref()
        .filter(|(_, value)| value == "absolute" || value == "fixed")
        .map(position),
      "vertical-align" => self
        .display
        .as_ref()
        .filter(|(_, keywords)| Self::is_block_level(keywords))
        .map(display),
      "table-layout" => self
        .display
        .as_ref()
        .filter(|(_, keywords)| {
          !keywords
            .iter()
            .any(|word| word == "table" || word == "inline-table")
        })
        .map(display),
      _ => None,
    }
  }
}

/// Whether the block pulls in declarations the rule cannot see: a Sass
/// `@include` or `@extend`, a Tailwind `@apply`, or a Less mixin call.
fn has_hidden_declarations(rule: &StyleRule, ctx: &RuleContext) -> bool {
  let at_rule = rule.nested_at_rules.iter().any(|node| {
    matches!(node, CssNode::AtRule(at)
      if matches!(at.name.to_ascii_lowercase().as_str(), "include" | "extend" | "apply"))
  });
  if at_rule {
    return true;
  }
  // Mixins are not always parsed as at-rules; look at the block's text.
  let start = rule.span.offset;
  let end = start + rule.span.length;
  let text = ctx.source_slice(start, end).unwrap_or("");
  let body = text.split_once('{').map_or("", |(_, body)| body);
  body.contains("@include")
    || body.contains("@extend")
    || body.contains("@apply")
    || body.split(';').any(|statement| {
      let statement = statement.trim_start();
      (statement.starts_with('.') || statement.starts_with('#'))
        && !statement.contains('{')
        && !statement.contains(':')
    })
}

impl Rule for GaleNoIneffectiveDeclarations {
  fn name(&self) -> &'static str {
    "gale/no-ineffective-declarations"
  }

  fn description(&self) -> &'static str {
    "Disallow declarations that another declaration in the block switches off"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports each declaration in a style rule's own block that the block's
  /// `display` or `position` switches off.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    if rule.declarations.is_empty() || has_hidden_declarations(rule, ctx) {
      return vec![];
    }

    let block = Layout::read(&rule.declarations);
    let mut diags = Vec::new();
    for decl in &rule.declarations {
      let property = decl.property.to_ascii_lowercase();
      let value = decl.value.trim().to_ascii_lowercase();
      if CSS_WIDE_KEYWORDS.contains(&value.as_str()) || is_initial_value(&property, &value) {
        continue;
      }
      if let Some(cause) = block.switched_off_by(&property) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!(
              "Unexpected \"{}\", which has no effect with \"{cause}\"",
              decl.property
            ),
          )
          .severity(self.default_severity())
          .span(Span::new(decl.span.offset, decl.span.length)),
        );
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  /// The messages the rule reports for `source` in `syntax`.
  fn messages(source: &str, syntax: Syntax) -> Vec<String> {
    crate::testing::lint(
      "gale/no-ineffective-declarations",
      json!(true),
      source,
      syntax,
    )
    .into_iter()
    .map(|d| d.message)
    .collect()
  }

  fn css(source: &str) -> Vec<String> {
    messages(source, Syntax::Css)
  }

  #[test]
  fn reports_container_properties_without_a_flex_or_grid_display() {
    assert_eq!(
      css(
        "a { display: block; justify-content: center; flex-wrap: wrap; grid-auto-flow: column; }"
      ),
      [
        "Unexpected \"justify-content\", which has no effect with \"display: block\"",
        "Unexpected \"flex-wrap\", which has no effect with \"display: block\"",
        "Unexpected \"grid-auto-flow\", which has no effect with \"display: block\"",
      ]
    );
  }

  #[test]
  fn reports_whatever_order_the_declarations_come_in() {
    assert_eq!(
      css("a { align-items: center; display: inline-block; }").len(),
      1
    );
  }

  #[test]
  fn reads_keywords_in_any_case() {
    assert_eq!(
      css("a { DISPLAY: Block; Justify-Content: center; }").len(),
      1
    );
    assert_eq!(css("a { position: STATIC; top: 0; }").len(), 1);
  }

  #[test]
  fn allows_flex_and_grid_containers() {
    for display in [
      "flex",
      "inline-flex",
      "grid",
      "inline-grid",
      "block flex",
      "inline grid",
    ] {
      assert!(
        css(&format!(
          "a {{ display: {display}; justify-content: center; gap: 1px; }}"
        ))
        .is_empty(),
        "{display}"
      );
    }
  }

  #[test]
  fn allows_displays_it_does_not_know() {
    for display in ["none", "contents", "-webkit-box", "var(--d)", "math"] {
      assert!(
        css(&format!(
          "a {{ display: {display}; justify-content: center; }}"
        ))
        .is_empty(),
        "{display}"
      );
    }
  }

  #[test]
  fn follows_the_cascade_inside_the_block() {
    assert!(css("a { display: block; display: flex; justify-content: center; }").is_empty());
    assert!(css("a { display: flex !important; display: block; gap: 1px; }").is_empty());
    assert_eq!(
      css("a { display: flex; display: block !important; gap: 1px; }").len(),
      1
    );
  }

  #[test]
  fn allows_gap_and_justify_content_in_multi_column_containers() {
    assert!(
      css("a { display: block; columns: 2; gap: 1rem; justify-content: center; }").is_empty()
    );
    assert!(css("a { display: block; column-width: 10em; column-gap: 1rem; }").is_empty());
  }

  #[test]
  fn reports_offsets_on_a_static_box() {
    assert_eq!(
      css("a { position: static; top: 0; inset: 1px; z-index: 2; }"),
      [
        "Unexpected \"top\", which has no effect with \"position: static\"",
        "Unexpected \"inset\", which has no effect with \"position: static\"",
      ]
    );
    assert!(css("a { position: relative; top: 0; }").is_empty());
    assert!(css("a { position: sticky; top: 0; }").is_empty());
  }

  #[test]
  fn reports_float_on_an_out_of_flow_box() {
    assert_eq!(css("a { position: absolute; float: left; }").len(), 1);
    assert_eq!(css("a { position: fixed; float: right; }").len(), 1);
    assert!(css("a { position: relative; float: left; }").is_empty());
  }

  #[test]
  fn reports_vertical_align_on_a_block_level_box() {
    for display in [
      "block",
      "flex",
      "grid",
      "flow-root",
      "list-item",
      "table",
      "block flow",
    ] {
      assert_eq!(
        css(&format!(
          "a {{ display: {display}; vertical-align: middle; }}"
        ))
        .len(),
        1,
        "{display}"
      );
    }
    for display in ["inline", "inline-block", "table-cell", "inline-flex"] {
      assert!(
        css(&format!(
          "a {{ display: {display}; vertical-align: middle; }}"
        ))
        .is_empty(),
        "{display}"
      );
    }
  }

  #[test]
  fn reports_table_layout_off_a_table() {
    assert_eq!(css("a { display: grid; table-layout: fixed; }").len(), 1);
    assert!(css("a { display: table; table-layout: fixed; }").is_empty());
    assert!(css("a { display: inline-table; table-layout: fixed; }").is_empty());
  }

  #[test]
  fn allows_resets_and_css_wide_keywords() {
    assert!(css("a { position: static; top: auto; inset: auto; float: none; }").is_empty());
    assert!(css("a { position: absolute; float: none; }").is_empty());
    assert!(
      css("a { display: block; justify-content: normal; vertical-align: baseline; }").is_empty()
    );
    assert!(css("a { display: block; justify-content: inherit; gap: revert-layer; }").is_empty());
  }

  #[test]
  fn allows_values_from_variables() {
    assert!(css("a { display: var(--display); justify-content: center; }").is_empty());
    assert!(css("a { position: var(--position); top: 0; }").is_empty());
  }

  #[test]
  fn leaves_nested_rules_to_their_own_block() {
    assert!(css("a { display: block; &:hover { justify-content: center; } }").is_empty());
  }

  #[test]
  fn skips_blocks_that_include_mixins() {
    let scss = |source| messages(source, Syntax::Scss);
    assert!(scss("a { @include row; display: block; justify-content: center; }").is_empty());
    assert!(scss("a { @extend %row; display: block; justify-content: center; }").is_empty());
    assert!(scss("a { display: $display; justify-content: center; }").is_empty());
    assert_eq!(
      scss("a { display: block; justify-content: center; }").len(),
      1
    );
    let less = |source| messages(source, Syntax::Less);
    assert!(less("a { .row(); display: block; justify-content: center; }").is_empty());
    assert!(less("a { display: @display; justify-content: center; }").is_empty());
  }
}
