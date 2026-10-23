//! Custom property definitions and `var()` references, as the custom
//! property rules read them: `csstools/value-no-unknown-custom-properties`,
//! `spectrum-tools/no-unknown-custom-properties` and
//! `plugin/no-{unknown,unused}-custom-properties`.

use std::collections::HashSet;

use crate::chars::is_word_or_hyphen_byte;

/// Extract all custom property definitions (`--name: value`) from a CSS source.
///
/// A text scan rather than a walk of the tree, as it also reads the
/// `importFrom` files (and `spectrum-tools/no-unknown-custom-properties`
/// the theme files) the same way.
pub fn collect_definitions_from_source(source: &str, props: &mut HashSet<String>) {
  let bytes = source.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i + 2 < len {
    if bytes[i] == b'-' && bytes[i + 1] == b'-' {
      let is_prop_start = if i == 0 {
        true
      } else {
        let prev = bytes[i - 1];
        prev == b' '
          || prev == b'\t'
          || prev == b'\n'
          || prev == b'\r'
          || prev == b'{'
          || prev == b';'
          || prev == b'('
      };

      if is_prop_start {
        let name_start = i;
        let j = custom_property_name_end(source, i + 2);
        let name = &source[name_start..j];

        let mut k = j;
        while k < len && (bytes[k] == b' ' || bytes[k] == b'\t') {
          k += 1;
        }

        if k < len && bytes[k] == b':' && (k + 1 >= len || bytes[k + 1] != b':') {
          props.insert(name.to_string());
        }

        i = j;
        continue;
      }
    }
    i += 1;
  }
}

/// A `var(--name)` reference in a value.
pub struct VarRef<'a> {
  /// The custom property's name, `--` included.
  pub name: &'a str,
  /// Where the name starts in the value.
  pub offset: usize,
  /// Whether anything but the closing `)` follows the name: a fallback,
  /// as the plugin counts one (any value-parser node after the name).
  pub has_fallback: bool,
}

/// Every `var(--name)` reference in a value, at all nesting levels: a `var(`
/// (in any case, not the tail of a longer name) whose first argument is a
/// `[\w-]+` name starting `--`.
pub fn var_references(value: &str) -> Vec<VarRef<'_>> {
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut refs = Vec::new();
  let mut i = 0;

  while i < len {
    if i + 4 <= len && bytes[i..i + 4].eq_ignore_ascii_case(b"var(") {
      if i > 0 && is_word_or_hyphen_byte(bytes[i - 1]) {
        i += 1;
        continue;
      }

      i += 4;

      while i < len && is_whitespace(bytes[i]) {
        i += 1;
      }

      if i + 1 < len && bytes[i] == b'-' && bytes[i + 1] == b'-' {
        let name_start = i;
        i = custom_property_name_end(value, i + 2);
        let name = &value[name_start..i];

        while i < len && is_whitespace(bytes[i]) {
          i += 1;
        }

        refs.push(VarRef {
          name,
          offset: name_start,
          has_fallback: i < len && bytes[i] != b')',
        });

        continue;
      }
    }

    i += 1;
  }

  refs
}

/// Where the custom property name whose `--` ends just before `start`
/// ends: past `[\w-]` characters, escapes (`--a\.5`) and SCSS interpolation
/// (`--#{$prefix}-a`), all of which a value parser reads as one word.
fn custom_property_name_end(text: &str, start: usize) -> usize {
  let bytes = text.as_bytes();
  let mut i = start;
  while i < bytes.len() {
    if is_word_or_hyphen_byte(bytes[i]) {
      i += 1;
    } else if bytes[i] == b'\\' && i + 1 < bytes.len() {
      i += 1 + text[i + 1..].chars().next().map_or(1, char::len_utf8);
    } else if bytes[i] == b'#' && bytes.get(i + 1) == Some(&b'{') {
      // The word runs to the `}`, or to a space inside the interpolation,
      // where the value parser splits it (`--a-#{'' + $b}`).
      match text[i..].find(['}', ' ', '\t', '\n', '\r']) {
        Some(end) if bytes[i + end] == b'}' => i += end + 1,
        Some(end) => return i + end,
        None => break,
      }
    } else {
      break;
    }
  }
  i
}

/// Whether the byte is CSS whitespace.
#[inline]
fn is_whitespace(b: u8) -> bool {
  b == b' ' || b == b'\t' || b == b'\n' || b == b'\r'
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn collects_definitions() {
    let source = ":root { --my-color: red; --spacing: 8px; }";
    let mut props = HashSet::new();
    collect_definitions_from_source(source, &mut props);
    assert!(props.contains("--my-color"));
    assert!(props.contains("--spacing"));
  }

  #[test]
  fn finds_var_references_with_their_fallbacks() {
    let found: Vec<(&str, usize, bool)> = var_references("Var(--a , x) calc(VAR(--b)) myvar(--c)")
      .iter()
      .map(|r| (r.name, r.offset, r.has_fallback))
      .collect();
    assert_eq!(found, [("--a", 4, true), ("--b", 22, false)]);
  }

  #[test]
  fn names_may_hold_escapes_and_interpolation() {
    let names: Vec<&str> = var_references("var(--a\\.5, x) var(--#{$p}-b)")
      .iter()
      .map(|r| r.name)
      .collect();
    assert_eq!(names, ["--a\\.5", "--#{$p}-b"]);
    // A space splits the word, and what follows counts as a fallback.
    let split: Vec<(&str, bool)> = var_references("var(--a-#{'' + $c}-5) var(--b )")
      .iter()
      .map(|r| (r.name, r.has_fallback))
      .collect();
    assert_eq!(split, [("--a-#{''", true), ("--b", false)]);
    let mut props = HashSet::new();
    collect_definitions_from_source("a { --a\\.5: 1; --#{$p}-b: 2; }", &mut props);
    assert!(props.contains("--a\\.5") && props.contains("--#{$p}-b"));
  }
}
