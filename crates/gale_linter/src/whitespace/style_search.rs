//! A port of `style-search` 0.1, the scanner Stylelint's whitespace rules
//! find their characters with: it walks a selector, value or params, keeps
//! track of comments, strings, parentheses and function arguments, and
//! reports each occurrence of a target string with where it sits.
//!
//! The port keeps the scanner's quirks, since the rules' results depend on
//! them: `//` starts a comment anywhere, even in plain CSS
//! (`url(http://x)`), a `(` at the very start of the text opens function
//! arguments (it tests the letters of `undefined`), and a quote right after
//! a backslash neither opens nor closes a string.

/// What to do with one kind of syntax: skip it, look inside it, or look
/// only inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
  Skip,
  Check,
  Only,
}

/// The string or strings to find.
#[derive(Debug, Clone, Copy)]
pub enum Needle<'n> {
  /// One string (`target: ','`).
  One(&'n str),
  /// The first of several that matches at a position
  /// (`target: ['>=', '<=', ...]`).
  Any(&'n [&'n str]),
}

/// The scanner's options, with its defaults: comments, strings and function
/// names skipped, function arguments and parentheticals checked.
#[derive(Debug, Clone, Copy)]
pub struct SearchOptions<'n> {
  pub target: Needle<'n>,
  pub comments: Mode,
  pub strings: Mode,
  pub function_names: Mode,
  pub function_arguments: Mode,
  pub parentheticals: Mode,
}

impl<'n> SearchOptions<'n> {
  /// Find `target` with the default options.
  pub fn new(target: &'n str) -> Self {
    Self::find(Needle::One(target))
  }

  /// Find any of `targets` with the default options.
  pub fn any(targets: &'n [&'n str]) -> Self {
    Self::find(Needle::Any(targets))
  }

  fn find(target: Needle<'n>) -> Self {
    Self {
      target,
      comments: Mode::Skip,
      strings: Mode::Skip,
      function_names: Mode::Skip,
      function_arguments: Mode::Check,
      parentheticals: Mode::Check,
    }
  }

  /// With function arguments skipped (`functionArguments: 'skip'`).
  pub fn skip_function_arguments(self) -> Self {
    Self {
      function_arguments: Mode::Skip,
      ..self
    }
  }

  /// Whether the target is exactly `c`, as `target === currentChar` tests
  /// it: never for a list of targets.
  fn is_exactly(&self, c: u8) -> bool {
    matches!(self.target, Needle::One(t) if t.len() == 1 && t.as_bytes()[0] == c)
  }
}

/// One occurrence of the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match<'n> {
  /// Byte offset of the match.
  pub start: usize,
  /// Byte offset just past it.
  pub end: usize,
  /// The target that matched.
  pub target: &'n str,
  pub inside_parens: bool,
  pub inside_function_arguments: bool,
  pub inside_comment: bool,
  pub inside_string: bool,
}

/// Call `callback` with every occurrence of the target in `source`.
pub fn style_search<'n>(
  source: &str,
  options: &SearchOptions<'n>,
  mut callback: impl FnMut(&Match<'n>),
) {
  let bytes = source.as_bytes();
  let at = |i: usize| bytes.get(i).copied();
  let before = |i: usize| i.checked_sub(1).and_then(|j| bytes.get(j).copied());

  let skip_comments = options.comments == Mode::Skip;
  let skip_strings = options.strings == Mode::Skip;
  let skip_function_names = options.function_names == Mode::Skip;
  let only_function_names = options.function_names == Mode::Only;

  let mut inside_string = false;
  let mut inside_comment = false;
  let mut inside_single_line_comment = false;
  let mut inside_parens = false;
  let mut inside_function_arguments = false;
  let mut opening_paren_count = 0usize;
  let mut opening_quote = 0u8;

  let get_match = |i: usize, state: (bool, bool, bool, bool)| -> Option<Match<'n>> {
    let matches = |t: &str| bytes.get(i..i + t.len()) == Some(t.as_bytes());
    let target = match options.target {
      Needle::One(t) => matches(t).then_some(t),
      Needle::Any(targets) => targets.iter().copied().find(|t| matches(t)),
    }?;
    Some(Match {
      start: i,
      end: i + target.len(),
      target,
      inside_parens: state.0,
      inside_function_arguments: state.1,
      inside_comment: state.2,
      inside_string: state.3,
    })
  };

  let mut handle = |found: &Match<'n>| {
    let filtered = (options.parentheticals == Mode::Only && !found.inside_parens)
      || (options.parentheticals == Mode::Skip && found.inside_parens)
      || (options.function_arguments == Mode::Only && !found.inside_function_arguments)
      || (options.function_arguments == Mode::Skip && found.inside_function_arguments)
      || (options.strings == Mode::Only && !found.inside_string)
      || (options.comments == Mode::Only && !found.inside_comment);
    if !filtered {
      callback(found);
    }
  };

  for i in 0..bytes.len() {
    let c = bytes[i];
    let state = |p: bool, f: bool, c: bool, s: bool| (p, f, c, s);

    // The beginning of a comment.
    if !inside_string && !inside_comment && c == b'/' && before(i) != Some(b'\\') {
      if at(i + 1) == Some(b'*') {
        inside_comment = true;
        continue;
      }
      if at(i + 1) == Some(b'/') {
        inside_comment = true;
        inside_single_line_comment = true;
        continue;
      }
    }

    if inside_comment {
      // The end of a standard comment (`/*/` does not end one).
      if !inside_single_line_comment
        && c == b'*'
        && before(i) != Some(b'\\')
        && at(i + 1) == Some(b'/')
        && before(i) != Some(b'/')
      {
        inside_comment = false;
        continue;
      }
      // The end of a single-line comment.
      if inside_single_line_comment && c == b'\n' {
        inside_comment = false;
        inside_single_line_comment = false;
      }
      if skip_comments {
        continue;
      }
    }

    // The beginning of a string.
    if !inside_comment && !inside_string && (c == b'"' || c == b'\'') {
      if before(i) == Some(b'\\') {
        continue;
      }
      opening_quote = c;
      inside_string = true;
      if options.is_exactly(c)
        && let Some(found) = get_match(
          i,
          state(
            inside_parens,
            inside_function_arguments,
            inside_comment,
            true,
          ),
        )
      {
        handle(&found);
      }
      continue;
    }

    if inside_string {
      // The end of a string.
      if c == opening_quote {
        if before(i) == Some(b'\\') {
          continue;
        }
        inside_string = false;
        continue;
      }
      if skip_strings {
        continue;
      }
    }

    // The beginning of parentheses or function arguments.
    if !inside_string && !inside_comment && c == b'(' {
      opening_paren_count += 1;
      inside_parens = true;
      // `/[a-zA-Z]/.test(source[i - 1])`, which is true for `undefined`.
      if before(i).is_none_or(|b| b.is_ascii_alphabetic()) {
        inside_function_arguments = true;
      }
      if options.is_exactly(b'(')
        && let Some(found) = get_match(
          i,
          state(
            true,
            inside_function_arguments,
            inside_comment,
            inside_string,
          ),
        )
      {
        handle(&found);
      }
      continue;
    }

    if inside_parens && c == b')' {
      opening_paren_count = opening_paren_count.saturating_sub(1);
      // Still inside the function when the match is made.
      if options.is_exactly(b')')
        && let Some(found) = get_match(
          i,
          state(
            inside_parens,
            inside_function_arguments,
            inside_comment,
            inside_string,
          ),
        )
      {
        handle(&found);
      }
      if opening_paren_count == 0 {
        inside_parens = false;
        inside_function_arguments = false;
      }
      continue;
    }

    // `/^[a-zA-Z]*\(/.test(source.slice(i))`.
    let letters = bytes[i..]
      .iter()
      .take_while(|b| b.is_ascii_alphabetic())
      .count();
    let is_function_name = at(i + letters) == Some(b'(');
    if (skip_function_names && is_function_name) || (only_function_names && !is_function_name) {
      continue;
    }

    if let Some(found) = get_match(
      i,
      state(
        inside_parens,
        inside_function_arguments,
        inside_comment,
        inside_string,
      ),
    ) {
      handle(&found);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn starts(source: &str, options: SearchOptions) -> Vec<usize> {
    let mut found = Vec::new();
    style_search(source, &options, |m| found.push(m.start));
    found
  }

  #[test]
  fn skips_comments_strings_and_function_names() {
    let commas = SearchOptions::new(",");
    assert_eq!(starts("a,/* , */b,'c,d'", commas), [1, 10]);
    assert_eq!(starts("a, // b, c\nd, e", commas), [1, 12]);
    let c = SearchOptions::new("c");
    assert_eq!(starts("calc(c) c", c), [5, 8]);
  }

  #[test]
  fn function_arguments_can_be_skipped() {
    let options = SearchOptions::new(",").skip_function_arguments();
    assert_eq!(
      starts("a, rgb(0, 0, 0), (b, c), d", options),
      [1, 15, 19, 23]
    );
    // A `(` that starts the text opens function arguments.
    assert_eq!(starts("(a, b), c", options), [6]);
  }

  #[test]
  fn several_targets_match_the_first_that_fits() {
    let operators = ["<=", "<", "="];
    let mut found = Vec::new();
    style_search(
      "(a <= b) and (c = d)",
      &SearchOptions::any(&operators),
      |m| {
        found.push((m.start, m.target));
      },
    );
    assert_eq!(found, [(3, "<="), (4, "="), (16, "=")]);
  }

  #[test]
  fn a_single_line_comment_runs_to_the_end_of_the_line_even_in_css() {
    let options = SearchOptions::new(",");
    assert_eq!(starts("url(http://a,b), c\nd, e", options), [20]);
  }
}
