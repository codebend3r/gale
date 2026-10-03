//! Rewriting a JavaScript object literal into JSON, one pass at a time.

/// Convert a JS object literal string to valid JSON.
///
/// Handles:
/// - Arrow function values → replaced with `null`
/// - Single-quoted strings → double-quoted
/// - Unquoted keys → double-quoted
/// - Trailing commas → removed
/// - Spread operator entries (`...foo`) → skipped
pub(super) fn js_object_to_json(js: &str) -> String {
  // Step 0: `require('pkg')` and `require.resolve('pkg')` become the string
  // `'pkg'`, so `extends: require.resolve('some-config')` still names the
  // config to resolve instead of collapsing to `null`.
  let s = inline_require_calls(js);

  // Step 0a: Replace arrow function expressions with null (before quote
  // conversion so we can still distinguish template literals).
  let s = replace_arrow_functions(&s);

  // Step 0b: Remove method calls on arrays/values (e.g. `.map( require.resolve )`).
  let s = remove_method_calls(&s);

  // Step 0c: Replace RegExp literals with their string pattern representation
  // (e.g. `/^[a-z]+$/i` → `"^[a-z]+$"`). Must run before quote conversion.
  let s = replace_regexp_literals(&s);

  // Step 1: Replace single-quoted strings with double-quoted strings.
  let s = convert_single_to_double_quotes(&s);

  // Step 1b: Concatenate adjacent string literals joined by `+`
  // (e.g. `"foo" + "bar"` → `"foobar"`). Must run after quote conversion.
  let s = concat_adjacent_strings(&s);

  // Step 2: Quote unquoted keys.
  let s = quote_unquoted_keys(&s);

  // Step 3: Remove trailing commas.
  let s = remove_trailing_commas(&s);

  // Step 4: Remove spread entries.
  let s = remove_spread_entries(&s);

  // Step 5: Replace bare identifier values with null.
  // After all other transformations, any remaining bare identifiers in value
  // positions (e.g. `"customSyntax": postcssScss`) are unresolved variable
  // references.  Replace them with `null` so the JSON is valid.
  replace_bare_identifier_values(&s)
}

/// Replace JavaScript RegExp literals with their pattern as a JSON string.
///
/// For example, `/^[a-z]+$/i` becomes `"^[a-z]+$"` (flags are dropped since
/// Stylelint regex options are always strings).
///
/// We distinguish RegExp `/` from division by checking the preceding non-
/// whitespace character: a regex can only appear after `:`, `[`, `,`, `(`, `=`,
/// `!`, `|`, `&`, `?`, `;`, `{`, or at the start of input — i.e. positions
/// where a value (not an operator) is expected.
fn replace_regexp_literals(s: &str) -> String {
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut result = String::with_capacity(len);
  let mut i = 0;

  let mut in_single = false;
  let mut in_double = false;
  let mut in_template = false;
  let mut escape_next = false;

  while i < len {
    let c = chars[i];

    if escape_next {
      result.push(c);
      escape_next = false;
      i += 1;
      continue;
    }

    if c == '\\' && (in_single || in_double || in_template) {
      result.push(c);
      escape_next = true;
      i += 1;
      continue;
    }

    // String state tracking
    if !in_double && !in_template && c == '\'' {
      in_single = !in_single;
      result.push(c);
      i += 1;
      continue;
    }
    if !in_single && !in_template && c == '"' {
      in_double = !in_double;
      result.push(c);
      i += 1;
      continue;
    }
    if !in_single && !in_double && c == '`' {
      in_template = !in_template;
      result.push(c);
      i += 1;
      continue;
    }
    if in_single || in_double || in_template {
      result.push(c);
      i += 1;
      continue;
    }

    // Check for line comments (//) and block comments (/*) — not regex.
    if c == '/' && i + 1 < len && (chars[i + 1] == '/' || chars[i + 1] == '*') {
      result.push(c);
      i += 1;
      continue;
    }

    // Detect potential regex literal.
    if c == '/' {
      // Look back at the last non-whitespace character in result.
      let prev = result.chars().rev().find(|ch| !ch.is_ascii_whitespace());
      let is_regex_context = match prev {
        None => true, // start of input
        Some(ch) => matches!(
          ch,
          ':'
            | '['
            | ','
            | '('
            | '='
            | '!'
            | '|'
            | '&'
            | '?'
            | ';'
            | '{'
            | '+'
            | '-'
            | '~'
            | '^'
            | '%'
            | '<'
            | '>'
        ),
      };

      if is_regex_context {
        // Parse the regex literal: collect chars until unescaped `/`.
        let mut pattern = String::new();
        i += 1; // skip opening `/`
        let mut regex_escape = false;
        let mut in_char_class = false;
        let mut valid = true;

        while i < len {
          let rc = chars[i];
          if regex_escape {
            pattern.push(rc);
            regex_escape = false;
            i += 1;
            continue;
          }
          if rc == '\\' {
            pattern.push(rc);
            regex_escape = true;
            i += 1;
            continue;
          }
          if rc == '[' {
            in_char_class = true;
            pattern.push(rc);
            i += 1;
            continue;
          }
          if rc == ']' && in_char_class {
            in_char_class = false;
            pattern.push(rc);
            i += 1;
            continue;
          }
          if rc == '/' && !in_char_class {
            i += 1; // skip closing `/`
            break;
          }
          if rc == '\n' {
            // Newline inside regex => not actually a regex.
            valid = false;
            break;
          }
          pattern.push(rc);
          i += 1;
        }

        if !valid || pattern.is_empty() {
          // Not a valid regex; emit the `/` and continue.
          result.push('/');
          // Reset i to after the opening `/` — we already advanced.
          // But `pattern` consumed chars, so we need to backtrack.
          // Simplest: just push the pattern back and the `/`.
          result.push_str(&pattern);
          continue;
        }

        // Skip optional flags (g, i, m, s, u, y, d, v)
        while i < len && chars[i].is_ascii_alphabetic() {
          i += 1;
        }

        // Emit the pattern as a double-quoted JSON string.
        // Escape any double-quotes and backslashes for JSON.
        result.push('"');
        for pc in pattern.chars() {
          if pc == '"' || pc == '\\' {
            result.push('\\');
          }
          result.push(pc);
        }
        result.push('"');
        continue;
      }
    }

    result.push(c);
    i += 1;
  }

  result
}

/// Concatenate adjacent string literals joined by `+`.
///
/// After quote conversion, strings are double-quoted.  This function finds
/// patterns like `"foo" + "bar"` and merges them into `"foobar"`.
fn concat_adjacent_strings(s: &str) -> String {
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut result = String::with_capacity(len);
  let mut i = 0;

  while i < len {
    let c = chars[i];

    // When we see a closing `"`, check if a `+` follows with another string.
    if c == '"' {
      // Collect the entire string (including opening quote).
      result.push('"');
      i += 1;
      // Scan to closing quote
      while i < len {
        let sc = chars[i];
        if sc == '\\' && i + 1 < len {
          result.push(sc);
          result.push(chars[i + 1]);
          i += 2;
          continue;
        }
        if sc == '"' {
          // Before pushing closing quote, check for `+ "..."`
          let mut j = i + 1;
          // Skip whitespace
          while j < len && chars[j].is_ascii_whitespace() {
            j += 1;
          }
          if j < len && chars[j] == '+' {
            j += 1;
            while j < len && chars[j].is_ascii_whitespace() {
              j += 1;
            }
            if j < len && chars[j] == '"' {
              // Skip the closing `"`, `+`, whitespace, and opening `"`
              // of the next string — effectively merging them.
              i = j + 1;
              continue; // continue scanning the merged string
            }
          }
          // No concatenation — emit closing quote.
          result.push('"');
          i += 1;
          break;
        }
        result.push(sc);
        i += 1;
      }
      continue;
    }

    result.push(c);
    i += 1;
  }

  result
}

/// Replace `require('x')` and `require.resolve('x')` with the string literal
/// `'x'`, keeping its quotes.
///
/// Only a call whose sole argument is a plain string literal is rewritten;
/// anything else (`require(path)`, `require('a' + b)`) is left for the later
/// steps, which turn it into `null`.
fn inline_require_calls(s: &str) -> String {
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut result = String::with_capacity(s.len());
  let mut i = 0;
  let mut quote: Option<char> = None;

  while i < len {
    let c = chars[i];
    if let Some(q) = quote {
      result.push(c);
      if c == '\\' && i + 1 < len {
        result.push(chars[i + 1]);
        i += 2;
        continue;
      }
      if c == q {
        quote = None;
      }
      i += 1;
      continue;
    }
    if matches!(c, '\'' | '"' | '`') {
      quote = Some(c);
      result.push(c);
      i += 1;
      continue;
    }

    let starts_word =
      i == 0 || !(chars[i - 1].is_alphanumeric() || matches!(chars[i - 1], '_' | '$' | '.'));
    if starts_word && let Some((literal, end)) = require_call_literal(&chars, i) {
      result.push_str(&literal);
      i = end;
      continue;
    }

    result.push(c);
    i += 1;
  }

  result
}

/// If `chars[start..]` is `require(<string>)` or `require.resolve(<string>)`,
/// the string literal (with its quotes) and the index just past the `)`.
fn require_call_literal(chars: &[char], start: usize) -> Option<(String, usize)> {
  let word = |at: usize, text: &str| {
    let end = at + text.chars().count();
    (end <= chars.len() && chars[at..end].iter().copied().eq(text.chars())).then_some(end)
  };
  let skip_space = |mut at: usize| {
    while at < chars.len() && chars[at].is_whitespace() {
      at += 1;
    }
    at
  };

  let mut i = word(start, "require")?;
  if let Some(after) = word(i, ".resolve") {
    i = after;
  }
  i = skip_space(i);
  if chars.get(i) != Some(&'(') {
    return None;
  }
  i = skip_space(i + 1);
  let quote = *chars.get(i).filter(|c| matches!(c, '\'' | '"' | '`'))?;
  let literal_start = i;
  i += 1;
  while i < chars.len() && chars[i] != quote {
    if chars[i] == '\\' || (quote == '`' && chars[i] == '$') {
      // Escapes and template substitutions are more than a plain name.
      return None;
    }
    i += 1;
  }
  if i >= chars.len() {
    return None;
  }
  let literal: String = chars[literal_start..=i].iter().collect();
  i = skip_space(i + 1);
  (chars.get(i) == Some(&')')).then(|| (literal, i + 1))
}

/// Replace arrow function expressions with `null`.
///
/// Detects `=>` outside of strings and backtracks to remove the entire
/// parameter list, then skips the function body (which may be a template
/// literal, string, block `{…}`, or other expression up to the next `,` or
/// `}` at the same brace depth).
fn replace_arrow_functions(s: &str) -> String {
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut result = String::with_capacity(len);
  let mut i = 0;

  // Track whether we're inside a string so we don't match `=>` in strings.
  let mut in_single = false;
  let mut in_double = false;
  let mut in_template = false;
  let mut escape_next = false;

  while i < len {
    let c = chars[i];

    if escape_next {
      result.push(c);
      escape_next = false;
      i += 1;
      continue;
    }

    if c == '\\' && (in_single || in_double || in_template) {
      result.push(c);
      escape_next = true;
      i += 1;
      continue;
    }

    // String state tracking
    if !in_double && !in_template && c == '\'' {
      in_single = !in_single;
      result.push(c);
      i += 1;
      continue;
    }
    if !in_single && !in_template && c == '"' {
      in_double = !in_double;
      result.push(c);
      i += 1;
      continue;
    }
    if !in_single && !in_double && c == '`' {
      in_template = !in_template;
      result.push(c);
      i += 1;
      continue;
    }
    if in_single || in_double || in_template {
      result.push(c);
      i += 1;
      continue;
    }

    // Detect `=>` outside of strings
    if c == '=' && i + 1 < len && chars[i + 1] == '>' {
      // Backtrack in `result` to remove the arrow function parameter list.
      // The params end with `)` (possibly with whitespace before `=>`),
      // or it's a bare identifier.
      // Remove trailing whitespace first.
      let trimmed = result.trim_end().len();
      result.truncate(trimmed);

      // Capture the parameter name while removing it from result.
      let param_name: String = if result.ends_with(')') {
        // Remove the parenthesized parameter list by finding the matching `(`.
        let mut removed = Vec::new();
        let mut depth = 0i32;
        while let Some(ch) = result.pop() {
          if ch == ')' {
            depth += 1;
          } else if ch == '(' {
            depth -= 1;
            if depth == 0 {
              break;
            }
          }
          if depth > 0 {
            removed.push(ch);
          }
        }
        removed.reverse();
        removed.iter().collect::<String>().trim().to_string()
      } else {
        // Bare identifier: remove word characters
        let mut removed = Vec::new();
        while result.ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$') {
          if let Some(ch) = result.pop() {
            removed.push(ch);
          }
        }
        removed.reverse();
        removed.iter().collect()
      };

      // Skip past `=>`
      i += 2;

      // Skip whitespace
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }

      // Check if body is a template literal
      if i < len && chars[i] == '`' {
        // Extract template literal content
        i += 1; // skip opening backtick
        let mut template_content = String::new();
        let mut tmpl_depth = 0i32;
        while i < len {
          if chars[i] == '\\' && i + 1 < len {
            template_content.push(chars[i]);
            template_content.push(chars[i + 1]);
            i += 2;
            continue;
          }
          if chars[i] == '$' && i + 1 < len && chars[i + 1] == '{' {
            tmpl_depth += 1;
            template_content.push('$');
            template_content.push('{');
            i += 2;
            continue;
          }
          if chars[i] == '}' && tmpl_depth > 0 {
            tmpl_depth -= 1;
            template_content.push('}');
            i += 1;
            continue;
          }
          if chars[i] == '`' && tmpl_depth == 0 {
            i += 1; // skip closing backtick
            break;
          }
          template_content.push(chars[i]);
          i += 1;
        }

        // Replace ${paramName} with ${name} to standardize placeholder
        let standardized = if !param_name.is_empty() && param_name != "name" {
          let placeholder = format!("${{{}}}", param_name);
          template_content.replace(&placeholder, "${name}")
        } else {
          template_content
        };

        // Emit as a JSON string with the template's value.
        result.push('"');
        result.push_str(&js_string_body_to_json(&standardized));
        result.push('"');
      } else {
        // Skip the arrow function body.
        if i < len {
          i = skip_js_expression(&chars, i);
        }

        // Emit `null` in place of the arrow function.
        result.push_str("null");
      }
      continue;
    }

    result.push(c);
    i += 1;
  }

  result
}

/// Skip a single JS expression starting at position `i`.
/// Returns the index just past the expression.
/// Handles:
/// - Template literals (`` ` ... ` ``)
/// - String literals (`'...'` or `"..."`)
/// - Block bodies (`{ ... }`)
/// - Array literals (`[ ... ]`)
/// - Parenthesized expressions (`( ... )`)
/// - Simple expressions (until `,`, `}`, `]` at depth 0, or newline
///   followed by a key-like pattern)
fn skip_js_expression(chars: &[char], start: usize) -> usize {
  let len = chars.len();
  let mut i = start;

  if i >= len {
    return i;
  }

  match chars[i] {
    '`' => {
      // Template literal — skip to matching backtick, handling `${…}`.
      i += 1;
      let mut tmpl_depth = 0i32; // for nested `${…}`
      while i < len {
        if chars[i] == '\\' {
          i += 2; // skip escaped char
          continue;
        }
        if chars[i] == '$' && i + 1 < len && chars[i + 1] == '{' {
          tmpl_depth += 1;
          i += 2;
          continue;
        }
        if chars[i] == '}' && tmpl_depth > 0 {
          tmpl_depth -= 1;
          i += 1;
          continue;
        }
        if chars[i] == '`' && tmpl_depth == 0 {
          i += 1; // past closing backtick
          break;
        }
        i += 1;
      }
    }
    '\'' | '"' => {
      let quote = chars[i];
      i += 1;
      while i < len {
        if chars[i] == '\\' {
          i += 2;
          continue;
        }
        if chars[i] == quote {
          i += 1;
          break;
        }
        i += 1;
      }
    }
    '{' => {
      i = skip_balanced(chars, i, '{', '}');
    }
    '(' => {
      i = skip_balanced(chars, i, '(', ')');
    }
    '[' => {
      i = skip_balanced(chars, i, '[', ']');
    }
    _ => {
      // Simple expression — advance until `,`, `}`, `]` at depth 0.
      let mut depth = 0i32;
      while i < len {
        match chars[i] {
          '(' | '[' | '{' => depth += 1,
          ')' | ']' | '}' => {
            if depth == 0 {
              break;
            }
            depth -= 1;
          }
          ',' if depth == 0 => break,
          _ => {}
        }
        i += 1;
      }
    }
  }

  i
}

/// Skip a balanced pair of delimiters (e.g. `{…}`, `(…)`, `[…]`),
/// respecting strings and nesting. Returns the index just past the
/// closing delimiter.
fn skip_balanced(chars: &[char], start: usize, open: char, close: char) -> usize {
  let len = chars.len();
  let mut i = start;
  let mut depth = 0i32;
  let mut in_sq = false;
  let mut in_dq = false;
  let mut in_tmpl = false;
  let mut esc = false;

  while i < len {
    let c = chars[i];
    if esc {
      esc = false;
      i += 1;
      continue;
    }
    if c == '\\' && (in_sq || in_dq || in_tmpl) {
      esc = true;
      i += 1;
      continue;
    }
    if !in_dq && !in_tmpl && c == '\'' {
      in_sq = !in_sq;
    } else if !in_sq && !in_tmpl && c == '"' {
      in_dq = !in_dq;
    } else if !in_sq && !in_dq && c == '`' {
      in_tmpl = !in_tmpl;
    }
    if !in_sq && !in_dq && !in_tmpl {
      if c == open {
        depth += 1;
      } else if c == close {
        depth -= 1;
        if depth == 0 {
          return i + 1;
        }
      }
    }
    i += 1;
  }
  i
}

/// Remove method calls like `.map(…)` or `.filter(…)` that appear after `]`
/// (array literals) or after identifiers. These are JS-only constructs that
/// have no JSON equivalent. The method call and its argument are simply
/// dropped so the preceding array value remains intact.
fn remove_method_calls(s: &str) -> String {
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut result = String::with_capacity(len);
  let mut i = 0;
  let mut in_single = false;
  let mut in_double = false;
  let mut in_template = false;
  let mut escape_next = false;

  while i < len {
    let c = chars[i];

    if escape_next {
      result.push(c);
      escape_next = false;
      i += 1;
      continue;
    }
    if c == '\\' && (in_single || in_double || in_template) {
      result.push(c);
      escape_next = true;
      i += 1;
      continue;
    }
    if !in_double && !in_template && c == '\'' {
      in_single = !in_single;
      result.push(c);
      i += 1;
      continue;
    }
    if !in_single && !in_template && c == '"' {
      in_double = !in_double;
      result.push(c);
      i += 1;
      continue;
    }
    if !in_single && !in_double && c == '`' {
      in_template = !in_template;
      result.push(c);
      i += 1;
      continue;
    }
    if in_single || in_double || in_template {
      result.push(c);
      i += 1;
      continue;
    }

    // Detect `.identifier(` pattern — a method call.
    if c == '.' && i + 1 < len && chars[i + 1].is_ascii_alphabetic() {
      // Check if this looks like a method call by scanning for an identifier
      // followed by `(`.
      let mut j = i + 1;
      while j < len && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
        j += 1;
      }
      // Skip whitespace between identifier and `(`
      let mut k = j;
      while k < len && chars[k].is_ascii_whitespace() {
        k += 1;
      }
      if k < len && chars[k] == '(' {
        // This is a method call — skip `.identifier(…)`
        let end = skip_balanced(&chars, k, '(', ')');
        i = end;
        continue;
      }
    }

    result.push(c);
    i += 1;
  }

  result
}

/// The body of a JavaScript string literal (the text between its quotes or
/// backticks, escapes and all) rewritten as the body of a JSON string with
/// the same value.
///
/// JavaScript allows what JSON does not: raw line breaks and tabs in a
/// template literal, escapes such as `` \` ``, `\'`, `\$`, `\x41`, `\v`
/// and `\u{1F600}`, line continuations, and needless escapes like `\d`.
/// Each becomes its JSON equivalent; an unescaped `"` gets a backslash.
pub(super) fn js_string_body_to_json(body: &str) -> String {
  let mut out = String::with_capacity(body.len() + 8);
  let mut chars = body.chars().peekable();
  while let Some(c) = chars.next() {
    match c {
      '\\' => match chars.next() {
        None => out.push_str("\\\\"),
        Some(escaped @ ('"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't')) => {
          out.push('\\');
          out.push(escaped);
        }
        Some('v') => out.push_str("\\u000b"),
        Some('0') if !chars.peek().is_some_and(char::is_ascii_digit) => {
          out.push_str("\\u0000");
        }
        Some('x') => {
          let hex: String = chars.by_ref().take(2).collect();
          match u32::from_str_radix(&hex, 16) {
            Ok(code) if hex.len() == 2 => out.push_str(&format!("\\u{code:04x}")),
            _ => out.push_str(&hex),
          }
        }
        Some('u') if chars.peek() == Some(&'{') => {
          chars.next();
          let hex: String = chars.by_ref().take_while(|&h| h != '}').collect();
          match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
            Some(decoded) => push_json_char(&mut out, decoded),
            None => out.push_str(&hex),
          }
        }
        Some('u') => out.push_str("\\u"),
        // A line continuation: the backslash and line break vanish.
        Some('\r') => {
          if chars.peek() == Some(&'\n') {
            chars.next();
          }
        }
        Some('\n' | '\u{2028}' | '\u{2029}') => {}
        // Any other escaped character stands for itself (`\'`, `` \` ``).
        Some(other) => push_json_char(&mut out, other),
      },
      other => push_json_char(&mut out, other),
    }
  }
  out
}

/// Append `c` to a JSON string body, escaping it if JSON requires.
fn push_json_char(out: &mut String, c: char) {
  match c {
    '"' => out.push_str("\\\""),
    '\\' => out.push_str("\\\\"),
    '\n' => out.push_str("\\n"),
    '\r' => out.push_str("\\r"),
    '\t' => out.push_str("\\t"),
    c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
    c => out.push(c),
  }
}

/// The body of the JavaScript string literal opening at `chars[start]`
/// (a `'`, `"` or backtick), and the index just past its closing quote.  An
/// unterminated literal runs to the end.
fn js_string_literal(chars: &[char], start: usize) -> (String, usize) {
  let quote = chars[start];
  let mut i = start + 1;
  let mut body = String::new();
  while i < chars.len() && chars[i] != quote {
    if chars[i] == '\\' && i + 1 < chars.len() {
      body.push(chars[i]);
      i += 1;
    }
    body.push(chars[i]);
    i += 1;
  }
  (body, (i + 1).min(chars.len()))
}

/// Rewrite every JavaScript string literal (single-quoted, double-quoted or
/// template) as a JSON string with the same value; see
/// [`js_string_body_to_json`].
fn convert_single_to_double_quotes(s: &str) -> String {
  let chars: Vec<char> = s.chars().collect();
  let mut result = String::with_capacity(s.len());
  let mut i = 0;
  while i < chars.len() {
    match chars[i] {
      // Every string form becomes a JSON string with the same value.
      // Template literals in config files are virtually always plain
      // strings; a `${...}` in one is kept as written.
      '\'' | '"' | '`' => {
        let (body, next) = js_string_literal(&chars, i);
        result.push('"');
        result.push_str(&js_string_body_to_json(&body));
        result.push('"');
        i = next;
      }
      c => {
        result.push(c);
        i += 1;
      }
    }
  }
  result
}

/// Add double quotes around unquoted object keys.
/// An unquoted key looks like `identifier:` or `identifier :` at the start
/// of a line or after `{` or `,`.
fn quote_unquoted_keys(s: &str) -> String {
  // Use a regex-like approach: scan for patterns like `word :` or `word:`
  // that are not inside strings.
  let mut result = String::with_capacity(s.len());
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut in_string = false;
  let mut escape_next = false;

  while i < len {
    let c = chars[i];

    if escape_next {
      result.push(c);
      escape_next = false;
      i += 1;
      continue;
    }

    if c == '\\' && in_string {
      result.push(c);
      escape_next = true;
      i += 1;
      continue;
    }

    if c == '"' {
      in_string = !in_string;
      result.push(c);
      i += 1;
      continue;
    }

    if in_string {
      result.push(c);
      i += 1;
      continue;
    }

    // Check if we're at the start of an unquoted key.
    // An unquoted key is an identifier ([a-zA-Z_$][a-zA-Z0-9_$-]*)
    // followed by optional whitespace and a `:`.
    if c.is_ascii_alphabetic() || c == '_' || c == '$' {
      // Check what came before — should be start, `{`, `,`, or whitespace/newline after one of those.
      let before_is_valid = {
        let trimmed_before = result.trim_end();
        trimmed_before.is_empty()
          || trimmed_before.ends_with('{')
          || trimmed_before.ends_with(',')
          || trimmed_before.ends_with('\n')
      };

      if before_is_valid {
        // Collect the identifier
        let key_start = i;
        while i < len
          && (chars[i].is_ascii_alphanumeric()
            || chars[i] == '_'
            || chars[i] == '$'
            || chars[i] == '-')
        {
          i += 1;
        }
        let key: String = chars[key_start..i].iter().collect();

        // Skip whitespace
        let ws_start = i;
        while i < len && chars[i].is_ascii_whitespace() {
          i += 1;
        }

        // Check for `:`
        if i < len && chars[i] == ':' {
          // It's an unquoted key — quote it
          result.push('"');
          result.push_str(&key);
          result.push('"');
          // Push any whitespace that was between key and `:`
          let ws: String = chars[ws_start..i].iter().collect();
          result.push_str(&ws);
          result.push(':');
          i += 1; // skip the `:`
          continue;
        } else {
          // Not a key — push as-is
          result.push_str(&key);
          let ws: String = chars[ws_start..i].iter().collect();
          result.push_str(&ws);
          continue;
        }
      }
    }

    result.push(c);
    i += 1;
  }
  result
}

/// Remove trailing commas before `}` or `]`.
fn remove_trailing_commas(s: &str) -> String {
  let mut result = String::with_capacity(s.len());
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut in_string = false;
  let mut escape_next = false;

  while i < len {
    let c = chars[i];

    if escape_next {
      result.push(c);
      escape_next = false;
      i += 1;
      continue;
    }

    if c == '\\' && in_string {
      result.push(c);
      escape_next = true;
      i += 1;
      continue;
    }

    if c == '"' {
      in_string = !in_string;
      result.push(c);
      i += 1;
      continue;
    }

    if in_string {
      result.push(c);
      i += 1;
      continue;
    }

    if c == ',' {
      // Look ahead past whitespace for `}` or `]`
      let mut j = i + 1;
      while j < len && chars[j].is_ascii_whitespace() {
        j += 1;
      }
      if j < len && (chars[j] == '}' || chars[j] == ']') {
        // Skip this comma (trailing comma)
        i += 1;
        continue;
      }
    }

    result.push(c);
    i += 1;
  }

  result
}

/// Remove spread operator entries like `...something` from object/array literals.
fn remove_spread_entries(s: &str) -> String {
  let mut result = String::with_capacity(s.len());
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut in_string = false;
  let mut escape_next = false;

  while i < len {
    let c = chars[i];

    if escape_next {
      result.push(c);
      escape_next = false;
      i += 1;
      continue;
    }

    if c == '\\' && in_string {
      result.push(c);
      escape_next = true;
      i += 1;
      continue;
    }

    if c == '"' {
      in_string = !in_string;
      result.push(c);
      i += 1;
      continue;
    }

    if in_string {
      result.push(c);
      i += 1;
      continue;
    }

    // Detect `...operand`: an identifier, a member chain such as
    // `...base.overrides`, a call, or a parenthesised expression such as
    // `...(cond ? { a: 1 } : {})`.
    if c == '.' && i + 2 < len && chars[i + 1] == '.' && chars[i + 2] == '.' {
      i = skip_spread_operand(&chars, i + 3);
      // Also skip a trailing comma if present
      // Skip whitespace first
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      if i < len && chars[i] == ',' {
        i += 1;
      }
      continue;
    }

    result.push(c);
    i += 1;
  }

  result
}

/// The index just past the operand of a spread that starts at `i`: an
/// identifier or a bracketed expression, followed by any number of `.member`
/// accesses and index expressions.
fn skip_spread_operand(chars: &[char], mut i: usize) -> usize {
  let len = chars.len();
  let skip_space = |mut i: usize| {
    while i < len && chars[i].is_whitespace() {
      i += 1;
    }
    i
  };
  i = skip_space(i);
  loop {
    match chars.get(i) {
      Some('(' | '[' | '{') => i = skip_bracketed(chars, i),
      Some(&c) if c.is_alphanumeric() || c == '_' || c == '$' => {
        while i < len && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$') {
          i += 1;
        }
      }
      _ => return i,
    }
    // What may follow: `.member`, `?.member`, a call or an index.
    let next = skip_space(i);
    match chars.get(next) {
      Some('.') if chars.get(next + 1) != Some(&'.') => i = skip_space(next + 1),
      Some('?') if chars.get(next + 1) == Some(&'.') => i = skip_space(next + 2),
      Some('(' | '[') => i = next,
      _ => return i,
    }
  }
}

/// The index just past the bracket that closes the one at `open`, skipping
/// quoted strings.  The end of the input when it never closes.
fn skip_bracketed(chars: &[char], open: usize) -> usize {
  let mut depth = 0usize;
  let mut i = open;
  while i < chars.len() {
    match chars[i] {
      quote @ ('"' | '\'' | '`') => {
        i += 1;
        while i < chars.len() && chars[i] != quote {
          if chars[i] == '\\' {
            i += 1;
          }
          i += 1;
        }
      }
      '(' | '[' | '{' => depth += 1,
      ')' | ']' | '}' => {
        depth -= 1;
        if depth == 0 {
          return i + 1;
        }
      }
      _ => {}
    }
    i += 1;
  }
  chars.len()
}

/// Replace bare identifier values with `null` in a JSON-like string.
///
/// After all other JS→JSON transformations, any remaining bare identifiers in
/// value positions (e.g. `"customSyntax": postcssScss`) are unresolved variable
/// references.  This function replaces them with `null` so that `serde_json`
/// can parse the result.
///
/// Only replaces identifiers that appear after `:` (value position) or as array
/// elements (after `[` or `,`).  Does not touch keys (already quoted), strings,
/// or JSON literals (`true`, `false`, `null`).
fn replace_bare_identifier_values(s: &str) -> String {
  let mut result = String::with_capacity(s.len());
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut in_string = false;
  let mut escape_next = false;

  while i < len {
    let c = chars[i];

    if escape_next {
      result.push(c);
      escape_next = false;
      i += 1;
      continue;
    }

    if c == '\\' && in_string {
      result.push(c);
      escape_next = true;
      i += 1;
      continue;
    }

    if c == '"' {
      in_string = !in_string;
      result.push(c);
      i += 1;
      continue;
    }

    if in_string {
      result.push(c);
      i += 1;
      continue;
    }

    // Check for a bare identifier in a value position.
    // Value positions come after `:`, after `[`, or after `,`.
    if (c.is_alphabetic() || c == '_' || c == '$') && !is_preceded_by_quote(&result) {
      // Read the full identifier.
      let start = i;
      while i < len
        && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$' || chars[i] == '.')
      {
        i += 1;
      }
      // `start` and `i` index `chars`, not bytes, so collect rather than
      // slice `s`: any multibyte character earlier in the config would
      // otherwise shift the slice or split a character.
      let ident: String = chars[start..i].iter().collect();
      // Preserve JSON literals.
      match ident.as_str() {
        "true" | "false" | "null" => result.push_str(&ident),
        _ => result.push_str("null"),
      }
      continue;
    }

    result.push(c);
    i += 1;
  }

  result
}

/// Check if the last non-whitespace character in `s` indicates we're in a
/// value position (after `:`, `[`, or `,`).
fn is_preceded_by_quote(s: &str) -> bool {
  // Returns true if the last non-whitespace char is `"`, indicating this
  // identifier is likely a key or part of a string context.
  // Returns false if it's `:`, `[`, `,`, or other value-context chars.
  let trimmed = s.trim_end();
  trimmed.ends_with('"')
}
