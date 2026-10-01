//! Ports of Stylelint's `isStandardSyntax*` helpers, which tell plain CSS
//! from preprocessor syntax (interpolation, variables, mixins) that the
//! rules skip.

/// Stylelint's `isStandardSyntaxSelector`: whether a rule's selector is
/// plain CSS rather than interpolation, a Sass placeholder or nested
/// property, a Less mixin or `:extend`, a template tag or a line comment.
/// Stylelint's selector rules skip the rules it rejects.
pub fn is_standard_syntax_selector(selector: &str) -> bool {
  !(has_interpolation(selector)
    || selector.starts_with('%')
    || selector.ends_with(':')
    || selector.contains(":extend")
    || has_less_mixin_call(selector)
    || (selector.ends_with(')') && !selector.contains(':'))
    || has_parametric_mixin(selector)
    || selector.contains("<%")
    || selector.contains("%>")
    || selector.contains("//"))
}

/// Stylelint's `hasInterpolation`: Less `@{x}`, SCSS `#{x}`, template
/// `{x}` or PostCSS-simple-vars `$(x)`.  The template form covers the
/// SCSS one.
pub fn has_interpolation(text: &str) -> bool {
  let bytes = text.as_bytes();
  // `/\{.+?\}/s`: a `{` with at least one character before a later `}`.
  let template = bytes
    .iter()
    .position(|&b| b == b'{')
    .is_some_and(|open| bytes[open + 1..].iter().skip(1).any(|&b| b == b'}'));
  // `/@\{.+?\}/` and `/\$\(.+?\)/`: on one line.
  let on_one_line = |opener: &str, close: u8| {
    text.match_indices(opener).any(|(i, _)| {
      let rest = &bytes[i + 2..];
      let line = rest
        .iter()
        .position(|&b| b == b'\n')
        .map_or(rest, |n| &rest[..n]);
      line.iter().skip(1).any(|&b| b == close)
    })
  };
  template || on_one_line("@{", b'}') || on_one_line("$(", b')')
}

/// `/\.[\w-]+\(.*\).+/`: a Less mixin call followed by more selector,
/// such as `.foo().bar`.
fn has_less_mixin_call(text: &str) -> bool {
  text.lines().any(|line| {
    let bytes = line.as_bytes();
    (0..bytes.len()).any(|dot| {
      if bytes[dot] != b'.' {
        return false;
      }
      let mut i = dot + 1;
      while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
      {
        i += 1;
      }
      i > dot + 1
        && bytes.get(i) == Some(&b'(')
        && bytes[i + 1..]
          .iter()
          .enumerate()
          .any(|(j, &b)| b == b')' && i + 2 + j < bytes.len())
    })
  })
}

/// `/\(@.*\)$/`: a Less parametric mixin such as `.mixin(@a: 1)`.
fn has_parametric_mixin(text: &str) -> bool {
  text.ends_with(')')
    && text
      .match_indices("(@")
      .any(|(i, _)| !text[i..].contains('\n'))
}

/// Stylelint's `isStandardSyntaxProperty`: not a Sass or Less variable, not
/// a Less `+`/`+_` merge, and free of interpolation.
pub fn is_standard_syntax_property(prop: &str) -> bool {
  !(prop.starts_with('$')
    || prop.starts_with('@')
    || prop.ends_with('+')
    || prop.ends_with("+_")
    || has_interpolation(prop))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn recognises_non_standard_selectors_as_stylelint_does() {
    for selector in ["a", "a:not(.b)", ".a > b::before", "a[href='{']"] {
      assert!(is_standard_syntax_selector(selector), "{selector}");
    }
    for selector in [
      ".a-#{$b}",
      ".a-@{b}",
      "%placeholder",
      "font:",
      "a:extend(.b)",
      ".mixin().b",
      ".mixin()",
      ".mixin(@a: 1)",
      "a // b",
      "<% a %>",
    ] {
      assert!(!is_standard_syntax_selector(selector), "{selector}");
    }
  }

  #[test]
  fn recognises_non_standard_properties() {
    assert!(is_standard_syntax_property("color"));
    assert!(is_standard_syntax_property("-webkit-box"));
    for prop in [
      "$foo",
      "@foo",
      "transform+",
      "transform+_",
      "#{$a}-b",
      "a-@{b}",
    ] {
      assert!(!is_standard_syntax_property(prop), "{prop}");
    }
  }
}
