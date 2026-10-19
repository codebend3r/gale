//! Units and keywords that can stand for any value.

use super::lookup;

/// Whether `name` is a standard CSS unit.
pub fn is_known_unit(name: &str) -> bool {
  lookup(KNOWN_UNITS, name)
}

/// The CSS-wide keywords every property accepts (Stylelint's
/// `basicKeywords`), lowercase.
pub const CSS_WIDE_KEYWORDS: &[&str] = &["initial", "inherit", "revert", "revert-layer", "unset"];

/// Whether `value` is a CSS-wide keyword, in any case.
pub fn is_css_wide_keyword(value: &str) -> bool {
  CSS_WIDE_KEYWORDS
    .iter()
    .any(|keyword| keyword.eq_ignore_ascii_case(value))
}

static KNOWN_UNITS: &[&str] = &[
  "%", "cap", "ch", "cm", "cqb", "cqh", "cqi", "cqmax", "cqmin", "cqw", "deg", "dpcm", "dpi",
  "dppx", "dvb", "dvh", "dvi", "dvmax", "dvmin", "dvw", "em", "ex", "fr", "grad", "hz", "ic", "in",
  "khz", "lh", "lvb", "lvh", "lvi", "lvmax", "lvmin", "lvw", "mm", "ms", "pc", "pt", "px", "q",
  "rad", "rcap", "rch", "rem", "rex", "ric", "rlh", "s", "svb", "svh", "svi", "svmax", "svmin",
  "svw", "turn", "vb", "vh", "vi", "vmax", "vmin", "vw", "x",
];

#[cfg(test)]
mod tests {
  use super::*;
  use crate::data::assert_sorted;

  #[test]
  fn known_units() {
    assert!(is_known_unit("px"));
    assert!(is_known_unit("rem"));
    assert!(is_known_unit("em"));
    assert!(is_known_unit("%"));
    assert!(!is_known_unit("xyz"));
  }

  #[test]
  fn css_wide_keywords_match_in_any_case() {
    assert!(is_css_wide_keyword("revert-layer"));
    assert!(is_css_wide_keyword("INHERIT"));
    assert!(!is_css_wide_keyword("auto"));
  }

  #[test]
  fn tables_are_sorted() {
    assert_sorted(KNOWN_UNITS, "KNOWN_UNITS");
  }
}
