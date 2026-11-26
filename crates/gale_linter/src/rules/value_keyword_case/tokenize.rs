//! The value tokenizer `value-keyword-case` reads declarations with: words,
//! functions, strings and `url()` contents, tracking function nesting.

use crate::chars::is_word_or_hyphen_byte;

/// One token of a value.
#[derive(Debug)]
pub(super) struct ValueToken {
  /// The token as written.
  pub(super) text: String,
  /// Where the token starts in the value.
  pub(super) offset: usize,
  pub(super) kind: TokenKind,
}

/// What a [`ValueToken`] is.
#[derive(Debug, PartialEq)]
pub(super) enum TokenKind {
  Ident,
  Function,
  String,
  UrlContent,
  Important,
  Other,
}

/// Tokenize a CSS value string, tracking function nesting.
pub(super) fn tokenize_value(value: &str) -> Vec<ValueToken> {
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut tokens = Vec::new();
  let mut i = 0;
  let mut func_stack: Vec<String> = Vec::new();

  while i < len {
    let b = bytes[i];

    // Whitespace
    if b.is_ascii_whitespace() {
      i += 1;
      continue;
    }

    // Punctuation that isn't meaningful
    if b == b','
      || b == b'/' && !(i + 1 < len && (bytes[i + 1] == b'*' || bytes[i + 1] == b'/'))
      || b == b'+'
      || b == b'*'
      || b == b'='
    {
      i += 1;
      continue;
    }

    // Opening paren without preceding ident
    if b == b'(' {
      func_stack.push(String::new());
      i += 1;
      continue;
    }

    // Closing paren
    if b == b')' {
      func_stack.pop();
      i += 1;
      continue;
    }

    // Square brackets (grid line names): `[name]` is one word to
    // postcss-value-parser, brackets included.  With whitespace inside,
    // the names are words of their own.
    if b == b'[' {
      let start = i;
      let end = value[i..]
        .find(|c: char| c == ']' || c.is_whitespace() || matches!(c, ',' | '(' | ')' | '/'))
        .map(|at| i + at);
      if let Some(end) = end.filter(|&end| bytes[end] == b']' && end > start + 1) {
        i = end + 1;
        tokens.push(ValueToken {
          text: value[start..i].to_string(),
          offset: start,
          kind: TokenKind::Ident,
        });
      } else {
        i += 1;
      }
      continue;
    }
    if b == b']' {
      i += 1;
      continue;
    }

    // String literal
    if b == b'"' || b == b'\'' {
      let quote = b;
      let start = i;
      i += 1;
      while i < len && bytes[i] != quote {
        if bytes[i] == b'\\' {
          i += 1;
        }
        i += 1;
      }
      if i < len {
        i += 1;
      }
      tokens.push(ValueToken {
        text: value[start..i].to_string(),
        offset: start,
        kind: TokenKind::String,
      });
      continue;
    }

    // CSS comment /* ... */
    if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
      i += 2;
      while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
        i += 1;
      }
      if i + 1 < len {
        i += 2;
      }
      continue;
    }

    // SCSS/Less single-line comment.
    // Inside parenthesised expressions (SCSS maps), PostCSS-SCSS treats
    // `//` (double slash) as a comment, but `///` (triple slash, SassDoc)
    // is NOT treated as a comment — identifiers after `///` remain visible
    // to rules like value-keyword-case.
    if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
      let is_triple = i + 2 < len && bytes[i + 2] == b'/';
      // At top level: always skip as comment.
      // Inside parens: skip `//` (double) as comment, but NOT `///` (triple).
      if func_stack.is_empty() || !is_triple {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }
      // `///` inside parens: skip the three slashes so the remaining
      // `//` is not re-interpreted as a double-slash comment.
      i += 3;
      continue;
    }

    // Hash (color or ID, or SCSS interpolation)
    if b == b'#' {
      i += 1;
      if i < len && bytes[i] == b'{' {
        let mut depth = 1;
        i += 1;
        while i < len && depth > 0 {
          if bytes[i] == b'{' {
            depth += 1;
          } else if bytes[i] == b'}' {
            depth -= 1;
          }
          i += 1;
        }
        continue;
      }
      while i < len && bytes[i].is_ascii_alphanumeric() {
        i += 1;
      }
      continue;
    }

    // `!word` (`!default`, a mistyped `!import`): one word to
    // postcss-value-parser, `!` included.  `!important` itself never gets
    // here, since PostCSS keeps it out of the value.
    if b == b'!' {
      let start = i;
      i += 1;
      while i < len && bytes[i].is_ascii_alphanumeric() {
        i += 1;
      }
      if i > start + 1 {
        tokens.push(ValueToken {
          text: value[start..i].to_string(),
          offset: start,
          kind: TokenKind::Important,
        });
      }
      continue;
    }

    // Number (possibly with unit), signed or not: postcss-value-parser's
    // `unit()` reads `-2px` as a dimension, which the rule skips.
    let starts_number = |at: usize| {
      bytes.get(at).is_some_and(u8::is_ascii_digit)
        || (bytes.get(at) == Some(&b'.') && bytes.get(at + 1).is_some_and(u8::is_ascii_digit))
    };
    if b == b'-' && starts_number(i + 1) {
      i += 1;
    }
    if starts_number(i) {
      while i < len
        && (bytes[i].is_ascii_digit()
          || bytes[i] == b'.'
          || bytes[i] == b'e'
          || bytes[i] == b'E'
          || bytes[i] == b'+'
          || bytes[i] == b'-'
          || bytes[i] == b'%')
      {
        i += 1;
      }
      // Skip unit
      while i < len && bytes[i].is_ascii_alphabetic() {
        i += 1;
      }
      continue;
    }

    // SCSS variable $...
    if b == b'$' {
      i += 1;
      while i < len && is_word_or_hyphen_byte(bytes[i]) {
        i += 1;
      }
      continue;
    }

    // Less variable @...
    if b == b'@' {
      i += 1;
      while i < len && is_word_or_hyphen_byte(bytes[i]) {
        i += 1;
      }
      continue;
    }

    // Semicolons, colons
    if b == b';' || b == b':' {
      i += 1;
      continue;
    }

    // A unicode range (`U+0400-045F`) is no word to postcss-value-parser.
    if (b == b'u' || b == b'U')
      && bytes.get(i + 1) == Some(&b'+')
      && bytes
        .get(i + 2)
        .is_some_and(|c| c.is_ascii_hexdigit() || *c == b'?')
    {
      i += 2;
      while i < len && (bytes[i].is_ascii_hexdigit() || bytes[i] == b'?' || bytes[i] == b'-') {
        i += 1;
      }
      continue;
    }

    // Identifier or function name
    if b.is_ascii_alphabetic() || b == b'-' || b == b'_' {
      let start = i;
      while i < len && is_word_or_hyphen_byte(bytes[i]) {
        i += 1;
      }
      // A Sass module member (`map.get(`, `theme.$color`) is one word.
      let mut module_member = false;
      while i + 1 < len
        && bytes[i] == b'.'
        && (bytes[i + 1].is_ascii_alphabetic() || matches!(bytes[i + 1], b'$' | b'_' | b'-'))
      {
        module_member = true;
        i += 1;
        while i < len
          && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'-' | b'_' | b'$'))
        {
          i += 1;
        }
      }
      let text = &value[start..i];
      if module_member && !(i < len && bytes[i] == b'(') {
        // `ns.$var` is not standard syntax, and Stylelint skips it.
        if !text.contains(".$") {
          tokens.push(ValueToken {
            text: text.to_string(),
            offset: start,
            kind: TokenKind::Ident,
          });
        }
        continue;
      }

      // Function call
      if i < len && bytes[i] == b'(' {
        let func_lower = text.to_ascii_lowercase();
        func_stack.push(func_lower.clone());
        i += 1;

        // For url(), skip content entirely
        if func_lower == "url" {
          let content_start = i;
          let mut depth = 1;
          while i < len && depth > 0 {
            if bytes[i] == b'(' {
              depth += 1;
            } else if bytes[i] == b')' {
              depth -= 1;
            }
            if depth > 0 {
              i += 1;
            }
          }
          tokens.push(ValueToken {
            text: value[content_start..i].to_string(),
            offset: content_start,
            kind: TokenKind::UrlContent,
          });
          if i < len {
            i += 1;
          }
          func_stack.pop();
          continue;
        }

        tokens.push(ValueToken {
          text: text.to_string(),
          offset: start,
          kind: TokenKind::Function,
        });
        continue;
      }

      // Regular identifier -- suppress if inside var()/attr()/counter()/counters()
      let in_suppressed_fn = func_stack
        .iter()
        .any(|f| f == "var" || f == "attr" || f == "counter" || f == "counters");

      tokens.push(ValueToken {
        text: text.to_string(),
        offset: start,
        kind: if in_suppressed_fn {
          TokenKind::Other
        } else {
          TokenKind::Ident
        },
      });
      continue;
    }

    i += 1;
  }

  tokens
}
