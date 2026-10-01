//! Declaration values and at-rule params exactly as the author wrote them.
//!
//! The CSS parser re-serialises declaration values (`background:
//! transparent` comes back as `none`, colours get shortened, ...) and
//! at-rule params, so a rule that compares, prints or points into them the
//! way Stylelint does must read them from the source instead.
//! [`declaration_value`] and [`at_rule_params`] give what PostCSS hands
//! Stylelint as `decl.value` and `atRule.params`, with their offsets.

use gale_css_parser::{AtRule, Declaration};

/// The value of `decl` as written in `source`, and its byte offset there.
///
/// Like PostCSS's `decl.value`: the text after the `:` with the surrounding
/// whitespace, the closing `;` and a trailing `!important` left out.  The
/// value ends at the first `;` or `}` outside strings, comments, brackets
/// and `#{...}` interpolation, so a `;` inside `url(data:...;base64,...)`
/// or a string does not cut it short.  A `{` that opens a block (SCSS
/// nested properties, `font: 12px { family: x }`) ends it too, except in a
/// custom property, whose value may hold braces.  Comments inside the value
/// are kept, where PostCSS would drop them.
///
/// `None` when the declaration's span does not point at `property: value`
/// in `source` (synthetic nodes, offsets past the end).
pub fn declaration_value<'a>(source: &'a str, decl: &Declaration) -> Option<(&'a str, usize)> {
  let start = decl.span.offset;
  let rest = source.get(start..)?;
  let colon = find_colon(rest)?;
  let after = &rest[colon + 1..];
  let leading = after.len() - after.trim_start().len();
  let value_start = start + colon + 1 + leading;
  let text = source.get(value_start..)?;
  let custom_property = decl.property.starts_with("--");
  let end = value_end(text, custom_property);
  let value = strip_important(text[..end].trim_end());
  Some((value, value_start))
}

/// The params of `at` as written in `source`, and their byte offset there.
///
/// Like PostCSS's `atRule.params`: the text after the at-rule name and the
/// whitespace that follows it, up to the `{` that opens the block or the
/// `;` that ends the statement (outside strings, comments, brackets and
/// interpolation), with trailing whitespace left out.
///
/// `None` when the at-rule's span does not start at `@name` in `source`.
pub fn at_rule_params<'a>(source: &'a str, at: &AtRule) -> Option<(&'a str, usize)> {
  let start = at.span.offset;
  let rest = source.get(start..)?.strip_prefix('@')?;
  let after_name = rest.get(at.name.len()..)?;
  if !rest[..at.name.len()].eq_ignore_ascii_case(&at.name) {
    return None;
  }
  let leading = after_name.len() - after_name.trim_start().len();
  let params_start = start + 1 + at.name.len() + leading;
  let text = source.get(params_start..)?;
  let end = value_end(text, false);
  Some((text[..end].trim_end(), params_start))
}

/// Offset of the `:` that ends the property name at the start of `text`,
/// skipping any `#{...}` interpolation in the name.  `None` when a `;`,
/// `{` or `}` comes first.
fn find_colon(text: &str) -> Option<usize> {
  let bytes = text.as_bytes();
  let mut depth = 0usize;
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'#' if bytes.get(i + 1) == Some(&b'{') => {
        depth += 1;
        i += 1;
      }
      b'}' if depth > 0 => depth -= 1,
      b':' if depth == 0 => return Some(i),
      b';' | b'{' | b'}' if depth == 0 => return None,
      _ => {}
    }
    i += 1;
  }
  None
}

/// Byte length of the value at the start of `text`: up to the first `;` or
/// `}` (or block-opening `{`, outside a custom property) that is not inside
/// a string, comment, brackets or interpolation.
fn value_end(text: &str, custom_property: bool) -> usize {
  let bytes = text.as_bytes();
  let mut depth = 0usize;
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      quote @ (b'"' | b'\'') => {
        i += 1;
        while i < bytes.len() && bytes[i] != quote {
          if bytes[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
      }
      b'/' if bytes.get(i + 1) == Some(&b'*') => {
        i = text[i + 2..]
          .find("*/")
          .map_or(bytes.len(), |close| i + 2 + close + 1);
      }
      b'#' if bytes.get(i + 1) == Some(&b'{') => {
        depth += 1;
        i += 1;
      }
      b'{' if depth == 0 && !custom_property => return i,
      b'(' | b'[' | b'{' => depth += 1,
      b')' | b']' => depth = depth.saturating_sub(1),
      b'}' if depth == 0 => return i,
      b'}' => depth -= 1,
      b';' if depth == 0 => return i,
      _ => {}
    }
    i += 1;
  }
  bytes.len()
}

/// `value` without a trailing `!important` (any case, with or without
/// whitespace after the `!`) and the whitespace before it.
fn strip_important(value: &str) -> &str {
  let lower = value.to_ascii_lowercase();
  let Some(without) = lower.strip_suffix("important") else {
    return value;
  };
  let without = without.trim_end();
  match without.strip_suffix('!') {
    Some(before) => value[..before.len()].trim_end(),
    None => value,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{CssNode, Syntax};

  /// The source value of every top-level rule declaration in `source`.
  fn values(source: &str, syntax: Syntax) -> Vec<(String, usize)> {
    let parsed = gale_css_parser::parse(source, syntax).expect("parses");
    let mut out = Vec::new();
    for node in &parsed.nodes {
      if let CssNode::Style(rule) = node {
        for decl in &rule.declarations {
          let (text, offset) = declaration_value(source, decl).expect("value");
          out.push((text.to_string(), offset));
        }
      }
    }
    out
  }

  #[test]
  fn reads_the_value_as_written_not_as_reserialised() {
    let source = "a { background: transparent; color: #FFFFFF; }";
    assert_eq!(
      values(source, Syntax::Css),
      vec![("transparent".to_string(), 16), ("#FFFFFF".to_string(), 36)]
    );
  }

  #[test]
  fn leaves_out_important_and_the_closing_brace() {
    let source = "a { color: red !important; margin: 0 ! IMPORTANT }";
    let got: Vec<String> = values(source, Syntax::Css)
      .into_iter()
      .map(|(text, _)| text)
      .collect();
    assert_eq!(got, vec!["red", "0"]);
  }

  #[test]
  fn semicolons_in_strings_and_urls_do_not_end_the_value() {
    let source = "a { background: url(data:image/png;base64,xx); content: \"a;b\"; }";
    let got: Vec<String> = values(source, Syntax::Css)
      .into_iter()
      .map(|(text, _)| text)
      .collect();
    assert_eq!(got, vec!["url(data:image/png;base64,xx)", "\"a;b\""]);
  }

  #[test]
  fn multiline_values_and_interpolation_are_kept_whole() {
    let source =
      "a {\n  color: color-mix(\n    in srgb,\n    red 50%,\n    blue);\n  --x-#{$y}: #{$z};\n}";
    let got: Vec<String> = values(source, Syntax::Scss)
      .into_iter()
      .map(|(text, _)| text)
      .collect();
    assert_eq!(
      got,
      vec!["color-mix(\n    in srgb,\n    red 50%,\n    blue)", "#{$z}"]
    );
  }

  #[test]
  fn reads_at_rule_params_as_written() {
    let source = "@container  sidebar  (min-width: 400px) {\n  a { color: red; }\n}\n@supports (content: \"{;\") {\n  a { color: red; }\n}\n";
    let parsed = gale_css_parser::parse(source, Syntax::Css).expect("parses");
    let params: Vec<(&str, usize)> = parsed
      .nodes
      .iter()
      .filter_map(|node| match node {
        CssNode::AtRule(at) => at_rule_params(source, at),
        _ => None,
      })
      .collect();
    assert_eq!(
      params,
      vec![
        ("sidebar  (min-width: 400px)", 12),
        ("(content: \"{;\")", 74)
      ]
    );
  }
}
