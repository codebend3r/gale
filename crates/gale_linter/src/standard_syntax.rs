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

/// Stylelint's `isStandardSyntaxValue`: not a Sass or Less variable (after
/// an optional leading operator), not a Sass module member, free of
/// interpolation, and no WebExtension `__MSG_name__` placeholder.
pub fn is_standard_syntax_value(value: &str) -> bool {
  let normalized = match value.as_bytes().first() {
    Some(b'-' | b'+' | b'*' | b'/') => &value[1..],
    _ => value,
  };
  if normalized.starts_with('$') || normalized.starts_with('@') {
    return false;
  }
  // `/^.+\.\$/` and `/^.+\.[-\w]+\(/`: a Sass module's variable or
  // function, on the first line.
  let first_line = value.lines().next().unwrap_or("");
  let bytes = first_line.as_bytes();
  let module_member = (1..bytes.len()).any(|dot| {
    if bytes[dot] != b'.' {
      return false;
    }
    if bytes.get(dot + 1) == Some(&b'$') {
      return true;
    }
    let mut i = dot + 1;
    while i < bytes.len()
      && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
    {
      i += 1;
    }
    i > dot + 1 && bytes.get(i) == Some(&b'(')
  });
  if module_member || has_interpolation(normalized) {
    return false;
  }
  // `/__MSG_\S+__/`
  !value.match_indices("__MSG_").any(|(i, _)| {
    let rest = &value[i + "__MSG_".len()..];
    let run = rest
      .find(char::is_whitespace)
      .map_or(rest, |end| &rest[..end]);
    // At least one character, then `__`.
    run
      .char_indices()
      .nth(1)
      .is_some_and(|(second, _)| run[second..].contains("__"))
  })
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
  fn recognises_non_standard_values() {
    for value in [
      "red",
      "1px solid",
      "-1px",
      "a.b",
      "url(x.png)",
      "__MSG_é",
      "__MSG___",
    ] {
      assert!(is_standard_syntax_value(value), "{value}");
    }
    for value in [
      "$a",
      "-$a",
      "@a",
      "ns.$a",
      "ns.fn(1)",
      "#{$a}",
      "__MSG_name__",
      "__MSG_é__",
    ] {
      assert!(!is_standard_syntax_value(value), "{value}");
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
