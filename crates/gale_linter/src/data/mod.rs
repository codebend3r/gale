//! Names and values CSS defines, which the rules check against.
//!
//! Each topic has its own file; the name tables are sorted for binary
//! search.

mod at_rules;
mod colors;
mod functions;
mod properties;
mod selectors;
mod values;

pub use at_rules::{
  is_known_at_rule, is_known_at_rule_for_syntax, is_known_media_feature, is_stylelint_at_keyword,
};
pub use colors::{is_named_color, named_color_for_rgb, named_color_rgb};
pub use functions::{MATH_FUNCTIONS, is_native_css_function};
pub use properties::{is_known_property, is_known_property_for};
pub use selectors::{
  is_experimental_html_element, is_known_html_element, is_known_pseudo_class,
  is_known_pseudo_element, is_legacy_pseudo_element,
};
pub use values::{CSS_WIDE_KEYWORDS, is_css_wide_keyword, is_known_unit};

/// Case-insensitive binary search over a sorted, lowercase table.
pub(crate) fn lookup(haystack: &[&str], needle: &str) -> bool {
  let lower = needle.to_ascii_lowercase();
  haystack.binary_search(&lower.as_str()).is_ok()
}

/// Asserts that `table` is in strictly increasing order, as the binary
/// search needs.
#[cfg(test)]
fn assert_sorted(table: &[&str], name: &str) {
  for window in table.windows(2) {
    assert!(
      window[0] < window[1],
      "{name}: '{0}' should come before '{1}'",
      window[0],
      window[1],
    );
  }
}
