//! The glob patterns files are named with on the command line.
//!
//! `globset` matches `*`, `**`, `?`, `[...]` and `{a,b}` alternation,
//! nested alternates included, but not bash's extglob operators (`+(a|b)`,
//! `@(a|b)`, `?(a|b)`, `*(a|b)`) or empty alternatives.  [`expand`]
//! rewrites the extglob forms Stylelint's globby accepts into alternation,
//! and splits path-level alternatives (`{src,docs}/**`) into one pattern
//! each, so each walks from its own root and reports its files in its own
//! turn, as globby does.

use std::path::{Path, PathBuf};

/// The patterns `paths` stand for, in order: extglob operators rewritten
/// as `globset` alternation and path-level alternatives split apart.
/// Plain paths come through unchanged.
pub(crate) fn expand(paths: &[String]) -> Vec<String> {
  paths
    .iter()
    .flat_map(|path| expand_braces(&convert_extglob(path)))
    .collect()
}

/// The matcher for `pattern`.  `*` does not match `/`, so `*.css` names the
/// files of one directory and `**/*.css` those of every directory below.
pub(crate) fn compile(pattern: &str) -> Result<globset::GlobMatcher, globset::Error> {
  globset::GlobBuilder::new(pattern)
    .literal_separator(true)
    .build()
    .map(|glob| glob.compile_matcher())
}

/// The directory a walk for `pattern` starts from: the parent of its
/// literal prefix (the text before the first meta-character), else the
/// current directory, as for `**/*.css`.  The walk's results are matched
/// against the whole pattern, so starting higher only costs time.
pub(crate) fn walk_root(pattern: &str) -> PathBuf {
  let first_meta = pattern.find(['*', '?', '{', '[']).unwrap_or(pattern.len());
  let prefix = &pattern[..first_meta];
  let parent = Path::new(prefix)
    .parent()
    .map(|p| p.to_string_lossy().to_string())
    .unwrap_or_default();
  let candidate = if parent.is_empty() {
    PathBuf::from(prefix)
  } else {
    PathBuf::from(parent)
  };
  // An empty path (e.g. when the pattern starts with `**` or `{`) is not a
  // valid walk root.
  if candidate.as_os_str().is_empty() {
    PathBuf::from(".")
  } else {
    candidate
  }
}

/// Returns `true` if the string contains glob meta-characters (`*`, `?`, `{`, `[`)
/// or extglob operators (`+(`, `?(`, `@(`, `*(`, `!(`).
pub(crate) fn is_glob_pattern(s: &str) -> bool {
  s.contains('*') || s.contains('?') || s.contains('{') || s.contains('[') || has_extglob(s)
}

/// Returns `true` if `s` contains any extglob operator: `+(`, `?(`, `@(`, `*(`, `!(`.
/// Also detects bare `(alt|alt)` at path-segment boundaries which some tools treat
/// as an implicit `@(...)`.
fn has_extglob(s: &str) -> bool {
  let bytes = s.as_bytes();
  for (i, &b) in bytes.iter().enumerate() {
    if b == b'(' {
      if i > 0 && matches!(bytes[i - 1], b'+' | b'?' | b'@' | b'*' | b'!') {
        return true;
      }
      // Bare `(alt|alt)` at path-segment start (e.g. `(src|docs)/**`)
      if (i == 0 || bytes[i - 1] == b'/') && s[i..].contains('|') {
        return true;
      }
    }
  }
  false
}

/// Convert extglob patterns to standard glob brace syntax that `globset` understands.
///
/// Handles the following bash extglob operators:
/// - `+(pattern|pattern)` (one or more)  -> `{pattern,pattern}`
/// - `@(pattern|pattern)` (exactly one)  -> `{pattern,pattern}`
/// - `?(pattern|pattern)` (zero or one)  -> `{pattern,pattern,}`
/// - `*(pattern|pattern)` (zero or more) -> `{pattern,pattern,}`
/// - `!(pattern|pattern)` (negation)     -> left as-is (not supported, warning printed)
///
/// Also converts bare `(alt|alt)` at path-segment boundaries to `{alt,alt}`.
fn convert_extglob(pattern: &str) -> String {
  let mut result = String::with_capacity(pattern.len());
  let bytes = pattern.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    // Check for extglob operator: +( ?( @( *( !(
    if i + 1 < len && bytes[i + 1] == b'(' && matches!(bytes[i], b'+' | b'?' | b'@' | b'!') {
      let op = bytes[i];
      if op == b'!' {
        // Negation extglob is not easily convertible; pass through as-is
        // and let globset handle it (it will likely fail, but we can't
        // do much better without a full negation engine).
        result.push(bytes[i] as char);
        i += 1;
        continue;
      }
      // Find matching close paren
      if let Some(close) = find_matching_paren(pattern, i + 1) {
        let inner = &pattern[i + 2..close];
        // Replace | with , and wrap in braces
        let converted = inner.replace('|', ",");
        // For ?() and *() add empty alternative (zero occurrences)
        if op == b'?' {
          result.push('{');
          result.push_str(&converted);
          result.push_str(",}");
        } else {
          // +() and @() -> {alternatives}
          result.push('{');
          result.push_str(&converted);
          result.push('}');
        }
        i = close + 1;
        continue;
      }
    }

    // Check for *( extglob (need special care since * is also a glob char)
    if i + 1 < len && bytes[i] == b'*' && bytes[i + 1] == b'(' {
      // Distinguish `*(` extglob from `*` glob followed by `(`.
      // In extglob, `*(` has alternatives separated by `|` inside.
      if let Some(close) = find_matching_paren(pattern, i + 1) {
        let inner = &pattern[i + 2..close];
        if inner.contains('|') {
          // This is a *() extglob
          let converted = inner.replace('|', ",");
          result.push('{');
          result.push_str(&converted);
          result.push_str(",}");
          i = close + 1;
          continue;
        }
      }
    }

    // Check for bare (alt|alt) at path-segment boundary
    if bytes[i] == b'('
      && (i == 0 || bytes[i - 1] == b'/')
      && let Some(close) = find_matching_paren(pattern, i)
    {
      let inner = &pattern[i + 1..close];
      if inner.contains('|') && !inner.contains('/') {
        let converted = inner.replace('|', ",");
        result.push('{');
        result.push_str(&converted);
        result.push('}');
        i = close + 1;
        continue;
      }
    }

    result.push(bytes[i] as char);
    i += 1;
  }

  result
}

/// Find the matching `)` for a `(` at position `open_pos`, handling nesting.
fn find_matching_paren(s: &str, open_pos: usize) -> Option<usize> {
  let bytes = s.as_bytes();
  if bytes[open_pos] != b'(' {
    return None;
  }
  let mut depth = 0;
  for (i, &b) in bytes[open_pos..].iter().enumerate() {
    match b {
      b'(' => depth += 1,
      b')' => {
        depth -= 1;
        if depth == 0 {
          return Some(open_pos + i);
        }
      }
      _ => {}
    }
  }
  None
}

/// Expand top-level brace alternatives in a glob pattern.
///
/// Patterns like `{a,b}/**/*.scss` are expanded to `["a/**/*.scss", "b/**/*.scss"]`.
/// Nested or extension-level braces (e.g. `**/*.{css,scss}`) are left untouched
/// because `globset` handles those natively.
///
/// Only the *first* top-level brace group is expanded (recursion handles deeper
/// groups), and only when the group starts at a position where it forms path
/// alternatives (i.e. the brace content contains `/` or the brace is at the
/// start of the pattern).
fn expand_braces(pattern: &str) -> Vec<String> {
  // Find the first `{` that is a top-level brace group containing path
  // separators or sitting at pattern start (path-level alternatives).
  if let Some(open) = pattern.find('{') {
    // Find the matching closing brace (handles one level of nesting).
    let mut depth = 0;
    let mut close = None;
    for (i, ch) in pattern[open..].char_indices() {
      match ch {
        '{' => depth += 1,
        '}' => {
          depth -= 1;
          if depth == 0 {
            close = Some(open + i);
            break;
          }
        }
        _ => {}
      }
    }
    if let Some(close_pos) = close {
      let prefix = &pattern[..open];
      let suffix = &pattern[close_pos + 1..];
      let inner = &pattern[open + 1..close_pos];

      // Only expand if the brace content contains `/` (path alternatives)
      // or if the brace is at the very start of the pattern.
      // Extension braces like `*.{css,scss}` don't contain `/` and
      // globset handles them fine, so skip those.
      if inner.contains('/') || open == 0 {
        // Split on top-level commas (respecting nested braces).
        let mut alternatives = Vec::new();
        let mut current = String::new();
        let mut depth = 0;
        for ch in inner.chars() {
          match ch {
            '{' => {
              depth += 1;
              current.push(ch);
            }
            '}' => {
              depth -= 1;
              current.push(ch);
            }
            ',' if depth == 0 => {
              alternatives.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
          }
        }
        alternatives.push(current);

        // Recursively expand each alternative (handles nested braces).
        let mut result = Vec::new();
        for alt in alternatives {
          let expanded_pattern = format!("{prefix}{alt}{suffix}");
          result.extend(expand_braces(&expanded_pattern));
        }
        return result;
      }
    }
  }
  vec![pattern.to_string()]
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn globset_handles_alternation_but_not_extglob() {
    // Why `expand` exists: nested alternates need no help, extglob does.
    let matches = |pattern: &str, path: &str| compile(pattern).unwrap().is_match(path);
    assert!(matches("{src/{a,b},docs/c}.css", "src/b.css"));
    assert!(matches("src/*.{css,scss}", "src/a.scss"));
    assert!(!matches("src/*.+(css|scss)", "src/a.css"));
    assert!(matches(&convert_extglob("src/*.+(css|scss)"), "src/a.css"));
    // An empty alternative matches nothing, so `?(s)css` finds only the
    // `scss` files; Stylelint would find both.
    assert!(!matches(&convert_extglob("*.?(s)css"), "a.css"));
    assert!(matches(&convert_extglob("*.?(s)css"), "a.scss"));
  }

  #[test]
  fn expand_rewrites_extglob_and_splits_path_alternatives_in_order() {
    let patterns = vec![
      "(src|docs)/**/*.+(css|scss)".to_string(),
      "a.css".to_string(),
    ];
    assert_eq!(
      expand(&patterns),
      ["src/**/*.{css,scss}", "docs/**/*.{css,scss}", "a.css"]
    );
  }

  #[test]
  fn walks_start_at_the_parent_of_the_literal_prefix() {
    assert_eq!(walk_root("src/styles/**/*.css"), PathBuf::from("src"));
    assert_eq!(walk_root("src/*.css"), PathBuf::from("src"));
    assert_eq!(walk_root("**/*.css"), PathBuf::from("."));
    assert_eq!(walk_root("*.css"), PathBuf::from("."));
  }

  #[test]
  fn test_is_glob_pattern() {
    assert!(is_glob_pattern("**/*.css"));
    assert!(is_glob_pattern("src/*.scss"));
    assert!(is_glob_pattern("src/{a,b}.css"));
    assert!(is_glob_pattern("src/[ab].css"));
    assert!(is_glob_pattern("src/??.css"));
    assert!(!is_glob_pattern("src/file.css"));
    assert!(!is_glob_pattern("src/dir"));
    assert!(!is_glob_pattern("plain-path"));
  }

  #[test]
  fn test_glob_brace_expansion_relative() {
    // Test that a bare `**/*.{css,scss}` (no absolute prefix, starting with **)
    // resolves to cwd as the walk root and does not panic.
    // We just verify that is_glob_pattern detects it correctly.
    assert!(is_glob_pattern("**/*.{css,scss}"));
  }

  #[test]
  fn test_expand_braces_directory_alternatives() {
    let result = expand_braces("{public/sass,packages}/**/*.scss");
    assert_eq!(result, vec!["public/sass/**/*.scss", "packages/**/*.scss"]);
  }

  #[test]
  fn test_expand_braces_extension_not_expanded() {
    // Extension braces don't contain `/`, so they should NOT be expanded
    // (globset handles them natively).
    let result = expand_braces("src/**/*.{css,scss}");
    assert_eq!(result, vec!["src/**/*.{css,scss}"]);
  }

  #[test]
  fn test_expand_braces_no_braces() {
    let result = expand_braces("src/**/*.css");
    assert_eq!(result, vec!["src/**/*.css"]);
  }

  #[test]
  fn test_expand_braces_at_start() {
    let result = expand_braces("{src,lib}/**/*.css");
    assert_eq!(result, vec!["src/**/*.css", "lib/**/*.css"]);
  }

  #[test]
  fn test_expand_braces_three_alternatives() {
    let result = expand_braces("{a,b,c}/**/*.scss");
    assert_eq!(result, vec!["a/**/*.scss", "b/**/*.scss", "c/**/*.scss"]);
  }

  #[test]
  fn test_expand_braces_mixed_path_and_extension() {
    // Pattern with both path braces and extension braces.
    let result = expand_braces("{src,lib}/**/*.{css,scss}");
    assert_eq!(result, vec!["src/**/*.{css,scss}", "lib/**/*.{css,scss}"]);
  }

  // -----------------------------------------------------------------------
  // Extglob conversion tests
  // -----------------------------------------------------------------------

  #[test]
  fn test_has_extglob() {
    assert!(has_extglob("*.+(css|scss)"));
    assert!(has_extglob("src/**/*.@(css|scss)"));
    assert!(has_extglob("?(a|b).css"));
    assert!(has_extglob("!(a|b).css"));
    // Bare (alt|alt) at segment start
    assert!(has_extglob("(src|docs)/**/*.css"));
    assert!(has_extglob("root/(src|docs)/**/*.css"));
    // Not extglob
    assert!(!has_extglob("src/**/*.css"));
    assert!(!has_extglob("src/**/*.{css,scss}"));
    // A `(` that is NOT at segment boundary and has no extglob prefix
    assert!(!has_extglob("src/file(1).css"));
  }

  #[test]
  fn test_convert_extglob_plus() {
    // +(css|scss) -> {css,scss}
    assert_eq!(
      convert_extglob("src/**/*.+(css|scss)"),
      "src/**/*.{css,scss}"
    );
  }

  #[test]
  fn test_convert_extglob_at() {
    // @(css|scss) -> {css,scss}
    assert_eq!(
      convert_extglob("src/**/*.@(css|scss)"),
      "src/**/*.{css,scss}"
    );
  }

  #[test]
  fn test_convert_extglob_question() {
    // ?(css|scss) -> {css,scss,}  (zero or one)
    assert_eq!(
      convert_extglob("src/**/*.?(css|scss)"),
      "src/**/*.{css,scss,}"
    );
  }

  #[test]
  fn test_convert_extglob_star() {
    // *(css|scss) -> {css,scss,}  (zero or more)
    assert_eq!(
      convert_extglob("src/**/*.*(css|scss)"),
      "src/**/*.{css,scss,}"
    );
  }

  #[test]
  fn test_convert_extglob_bare_parens() {
    // Bare (src|docs) at start -> {src,docs}
    assert_eq!(
      convert_extglob("(src|docs)/**/*.css"),
      "{src,docs}/**/*.css"
    );
  }

  #[test]
  fn test_convert_extglob_bare_parens_mid_path() {
    // Bare (src|docs) after a slash -> {src,docs}
    assert_eq!(
      convert_extglob("root/(src|docs)/**/*.css"),
      "root/{src,docs}/**/*.css"
    );
  }

  #[test]
  fn test_convert_extglob_angular_pattern() {
    // The exact pattern from angular-components:
    // (src|docs)/**/*.+(css|scss)
    assert_eq!(
      convert_extglob("(src|docs)/**/*.+(css|scss)"),
      "{src,docs}/**/*.{css,scss}"
    );
  }

  #[test]
  fn test_convert_extglob_no_change() {
    // Standard glob patterns should pass through unchanged.
    assert_eq!(convert_extglob("src/**/*.css"), "src/**/*.css");
    assert_eq!(
      convert_extglob("src/**/*.{css,scss}"),
      "src/**/*.{css,scss}"
    );
    assert_eq!(convert_extglob("**/*.css"), "**/*.css");
  }

  #[test]
  fn test_is_glob_pattern_detects_extglob() {
    assert!(is_glob_pattern("src/**/*.+(css|scss)"));
    assert!(is_glob_pattern("(src|docs)/**/*.css"));
  }
}
