//! Hex colors in declaration values.

use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::options::LetterCase;

/// Each `#` followed by hex digits in `value`, with its offset.
pub fn find_hex_colors(value: &str) -> Vec<(usize, String)> {
  let mut colors = Vec::new();
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;
  while i < len {
    if bytes[i] == b'#' {
      let start = i;
      i += 1;
      while i < len && (bytes[i] as char).is_ascii_hexdigit() {
        i += 1;
      }
      if i > start + 1 {
        colors.push((start, value[start..i].to_string()));
      }
    } else {
      i += 1;
    }
  }
  colors
}

/// Whether `word` is a hex color: `#` and 3, 4, 6 or 8 hex digits.
pub fn is_valid_hex(word: &str) -> bool {
  word.strip_prefix('#').is_some_and(|digits| {
    matches!(digits.len(), 3 | 4 | 6 | 8) && digits.bytes().all(|b| b.is_ascii_hexdigit())
  })
}

/// Every hex color in the declarations of style rules that is not in
/// `case`, reported for `rule` with a fix titled by `fix_title`, which is
/// given the case and the converted color.
///
/// `color-hex-case` and `@stylistic/color-hex-case` are the same check;
/// only their names and fix titles differ.
pub fn hex_case_diagnostics(
  tree: &PostcssTree,
  rule: &'static str,
  case: LetterCase,
  fix_title: fn(LetterCase, &str) -> String,
) -> Vec<Diagnostic> {
  let mut diags = Vec::new();
  // The declarations of style rules, read whole: property through `;`.
  for decl in tree.walk_decls() {
    if !decl.in_style_rule() {
      continue;
    }
    let decl_start = decl.span().offset;
    let search_area = decl.source_text();

    for (rel_offset, hex) in find_hex_colors(search_area) {
      if !case.is_wrong(&hex) {
        continue;
      }
      let expected = case.apply(&hex);
      let abs_offset = decl_start + rel_offset;
      diags.push(
        Diagnostic::new(rule, format!("Expected \"{hex}\" to be \"{expected}\""))
          .span(Span::new(abs_offset, hex.len()))
          .fix(Fix::new(
            fix_title(case, &expected),
            vec![Edit::new(Span::new(abs_offset, hex.len()), &expected)],
          )),
      );
    }
  }
  diags
}
