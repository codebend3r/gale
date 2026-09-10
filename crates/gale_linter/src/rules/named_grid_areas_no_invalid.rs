use std::collections::HashMap;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow invalid named grid areas in `grid-template-areas`.
///
/// Validates that:
/// - Each row string has the same number of cell tokens
/// - Named areas form contiguous rectangles
///
/// Equivalent to Stylelint's `named-grid-areas-no-invalid` rule.
pub struct NamedGridAreasNoInvalid;

impl Rule for NamedGridAreasNoInvalid {
  fn name(&self) -> &'static str {
    "named-grid-areas-no-invalid"
  }

  fn description(&self) -> &'static str {
    "Disallow invalid named grid areas"
  }

  fn default_severity(&self) -> Severity {
    Severity::Error
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Declaration(decl) = node else {
      return vec![];
    };

    let prop = decl.property.to_ascii_lowercase();
    if prop != "grid-template-areas" {
      return vec![];
    }

    let value = decl.value.trim();
    if value.eq_ignore_ascii_case("none") {
      return vec![];
    }

    // Parse rows: each quoted string is a row
    let rows = parse_grid_rows(value);
    if rows.is_empty() {
      return vec![];
    }

    let mut diagnostics = Vec::new();

    // Check all rows have the same number of columns
    let col_count = rows[0].len();
    for (i, row) in rows.iter().enumerate() {
      if row.len() != col_count {
        diagnostics.push(
          Diagnostic::new(
            self.name(),
            format!(
              "Expected row {} to have {} cell tokens but found {}",
              i + 1,
              col_count,
              row.len()
            ),
          )
          .severity(self.default_severity())
          .span(Span::new(decl.span.offset, decl.span.length)),
        );
        // If row counts don't match, skip rectangle check
        return diagnostics;
      }
    }

    // Check that each named area forms a rectangle.
    // Collect positions of each named area.
    let mut areas: HashMap<&str, Vec<(usize, usize)>> = HashMap::new();
    for (row_idx, row) in rows.iter().enumerate() {
      for (col_idx, cell) in row.iter().enumerate() {
        if *cell != "." {
          areas.entry(cell).or_default().push((row_idx, col_idx));
        }
      }
    }

    for (name, positions) in &areas {
      if !is_rectangle(positions) {
        diagnostics.push(
          Diagnostic::new(
            self.name(),
            format!("Named grid area \"{}\" does not form a rectangle", name),
          )
          .severity(self.default_severity())
          .span(Span::new(decl.span.offset, decl.span.length)),
        );
      }
    }

    diagnostics
  }
}

/// Parse grid-template-areas value into rows of cell tokens.
/// Each quoted string (single or double quotes) is a row.
/// Within each row, tokens are separated by whitespace.
fn parse_grid_rows(value: &str) -> Vec<Vec<&str>> {
  let mut rows = Vec::new();
  let mut chars = value.char_indices();

  while let Some((i, ch)) = chars.next() {
    if ch == '"' || ch == '\'' {
      let quote = ch;
      let start = i + 1;
      let mut end = start;
      for (j, c) in chars.by_ref() {
        if c == quote {
          end = j;
          break;
        }
      }
      let row_str = &value[start..end];
      let tokens: Vec<&str> = row_str.split_whitespace().collect();
      if !tokens.is_empty() {
        rows.push(tokens);
      }
    }
  }

  rows
}

/// Check if a set of (row, col) positions forms a contiguous rectangle.
fn is_rectangle(positions: &[(usize, usize)]) -> bool {
  if positions.is_empty() {
    return true;
  }

  let min_row = positions.iter().map(|p| p.0).min().unwrap();
  let max_row = positions.iter().map(|p| p.0).max().unwrap();
  let min_col = positions.iter().map(|p| p.1).min().unwrap();
  let max_col = positions.iter().map(|p| p.1).max().unwrap();

  let expected_count = (max_row - min_row + 1) * (max_col - min_col + 1);
  if positions.len() != expected_count {
    return false;
  }

  // Verify all positions within the bounding box are present
  for row in min_row..=max_row {
    for col in min_col..=max_col {
      if !positions.contains(&(row, col)) {
        return false;
      }
    }
  }

  true
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn decl(property: &str, value: &str) -> CssNode {
    CssNode::Declaration(Declaration {
      property: property.to_string(),
      value: value.to_string(),
      span: ParserSpan::new(0, 0),
      important: false,
    })
  }

  #[test]
  fn allows_valid_rectangle_areas() {
    let node = decl("grid-template-areas", r#""a a" "a a""#);
    let d = NamedGridAreasNoInvalid.check(&node, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_valid_multiple_named_areas() {
    let node = decl("grid-template-areas", r#""a b" "a b""#);
    let d = NamedGridAreasNoInvalid.check(&node, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_valid_with_null_cells() {
    let node = decl("grid-template-areas", r#""a . b" "a . b""#);
    let d = NamedGridAreasNoInvalid.check(&node, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_non_rectangle_area() {
    // "a" occupies (0,0), (0,1), (1,1) — not a rectangle
    let node = decl("grid-template-areas", r#""a a" "b a""#);
    let d = NamedGridAreasNoInvalid.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("does not form a rectangle"));
    assert!(d[0].message.contains("\"a\""));
  }

  #[test]
  fn reports_l_shaped_area() {
    // "a" occupies (0,0), (1,0), (1,1) — L-shape, not a rectangle
    let node = decl("grid-template-areas", r#""a b" "a a""#);
    let d = NamedGridAreasNoInvalid.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("does not form a rectangle"));
  }

  #[test]
  fn reports_unequal_row_lengths() {
    let node = decl("grid-template-areas", r#""a a" "b b b""#);
    let d = NamedGridAreasNoInvalid.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("cell tokens"));
  }

  #[test]
  fn allows_none_value() {
    let node = decl("grid-template-areas", "none");
    let d = NamedGridAreasNoInvalid.check(&node, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_other_properties() {
    let node = decl("display", "grid");
    let d = NamedGridAreasNoInvalid.check(&node, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_single_row() {
    let node = decl("grid-template-areas", r#""header header sidebar""#);
    let d = NamedGridAreasNoInvalid.check(&node, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_complex_valid_grid() {
    let node = decl(
      "grid-template-areas",
      r#""header header header" "nav content sidebar" "footer footer footer""#,
    );
    let d = NamedGridAreasNoInvalid.check(&node, &ctx());
    assert!(d.is_empty());
  }
}
