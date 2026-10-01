//! Regular expressions from rule options.
//!
//! Stylelint options are JavaScript regular expressions: a `*-pattern`
//! rule's primary option, or the `/.../` entries of an ignore or allow list.
//! Patterns written for JavaScript often use lookahead (`^(?!js-)`),
//! lookbehind or backreferences, which the `regex` crate rejects.  Everything
//! here compiles with `fancy_regex`, which supports them and hands plain
//! patterns to `regex` unchanged, so ordinary patterns cost nothing extra.
//!
//! Compiled patterns are cached, so a rule can look its pattern up on every
//! node without rebuilding it, and a pattern that does not compile is
//! reported once per file as an invalid option (see [`invalid_option`])
//! instead of quietly switching the rule off.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use gale_diagnostics::Diagnostic;

pub use fancy_regex::Regex;

thread_local! {
  /// Compiled patterns (or their compile errors), keyed by the option text.
  /// Per thread, so lookups on the hot path never contend for a lock.
  static CACHE: RefCell<HashMap<String, Result<Arc<Regex>, String>>> =
    RefCell::new(HashMap::new());
}

/// Compile a pattern option, or return why it does not compile.
///
/// `pattern` is either a regex source (`"^[a-z]+$"`) or a JavaScript regex
/// literal (`"/^[a-z]+$/i"`); in the literal form the `i`, `m` and `s` flags
/// apply and the others (`g`, `u`, `y`, ...) change nothing for a match test.
/// Results are cached per thread.
pub fn compile(pattern: &str) -> Result<Arc<Regex>, String> {
  CACHE.with(|cache| {
    if let Some(hit) = cache.borrow().get(pattern) {
      return hit.clone();
    }
    let compiled = build(pattern);
    cache
      .borrow_mut()
      .insert(pattern.to_string(), compiled.clone());
    compiled
  })
}

/// Compile `pattern` for a rule, turning a failure into Stylelint's
/// invalid-option report for that rule.
///
/// The usual shape at a call site is
/// `let re = match pattern::for_rule(self.name(), p) { Ok(re) => re, Err(d) => return vec![d] };`.
pub fn for_rule(rule_name: &str, pattern: &str) -> Result<Arc<Regex>, Diagnostic> {
  compile(pattern).map_err(|error| invalid_option(rule_name, pattern, &error))
}

/// Whether `pattern` matches `text`.
///
/// A match that gives up (`fancy_regex` caps backtracking) counts as no
/// match, which is what an ignore or allow list should do with it.
pub fn is_match(pattern: &Regex, text: &str) -> bool {
  pattern.is_match(text).unwrap_or(false)
}

/// `/source/flags` split into its source and flags, when `value` is written
/// as a JavaScript regex literal.  The source must not be empty and the
/// flags must be JavaScript ones.
///
/// List options use this to tell a regex entry (`"/^foo/"`) from a literal
/// string (`"foo"`), as Stylelint does for strings that start and end with
/// `/`.
pub fn split_literal(value: &str) -> Option<(&str, &str)> {
  let rest = value.strip_prefix('/')?;
  let close = rest.rfind('/')?;
  let (source, flags) = (&rest[..close], &rest[close + 1..]);
  (!source.is_empty() && flags.chars().all(|flag| "dgimsuvy".contains(flag)))
    .then_some((source, flags))
}

/// The compiled regex for a list entry written as a regex literal
/// (`"/^foo/"`, `"/^foo/i"`).
///
/// `None` for a plain string, and for a literal that does not compile: the
/// runner reports that one as an invalid option (see [`invalid_options`]),
/// and it matches nothing.
pub fn regex_entry(entry: &str) -> Option<Arc<Regex>> {
  split_literal(entry)?;
  compile(entry).ok()
}

/// Test `text` against a list entry written as a regex literal.
///
/// `Some(matched)` for such an entry (one that does not compile matches
/// nothing); `None` for a plain string, which the caller compares however
/// the rule compares names.
pub fn match_regex_entry(entry: &str, text: &str) -> Option<bool> {
  split_literal(entry)?;
  Some(compile(entry).is_ok_and(|re| is_match(&re, text)))
}

/// Stylelint's `matchesStringOrRegExp` for one list entry: an entry written
/// as a regex literal (`"/^foo/"`) is tested as a regex, any other entry
/// must equal `text` exactly.
pub fn matches_entry(entry: &str, text: &str) -> bool {
  match_regex_entry(entry, text).unwrap_or(entry == text)
}

/// Whether any entry of a string-or-list option matches `text`, the way
/// Stylelint's `optionsMatches` reads an option such as `ignoreAtRules`:
/// the option may be a single string or an array of them, and non-string
/// entries match nothing.
pub fn option_matches(option: Option<&serde_json::Value>, text: &str) -> bool {
  match option {
    Some(serde_json::Value::String(entry)) => matches_entry(entry, text),
    Some(serde_json::Value::Array(entries)) => entries
      .iter()
      .filter_map(serde_json::Value::as_str)
      .any(|entry| matches_entry(entry, text)),
    _ => false,
  }
}

/// Secondary options every rule accepts that never hold a pattern.
const NON_PATTERN_OPTIONS: &[&str] = &["message", "url", "severity"];

/// Invalid-option reports for every regex literal in a rule's options that
/// does not compile.
///
/// Walks the whole options value — list entries and object keys alike, as
/// in `ignoreProperties: ["/^my-/"]` or `{ "/^border/": [...] }` — so a
/// broken pattern is reported whichever option holds it.  Plain strings are
/// left alone: only a rule knows whether it treats one as a pattern (the
/// `*-pattern` rules report theirs through [`for_rule`]).
pub fn invalid_options(rule_name: &str, options: &serde_json::Value) -> Vec<Diagnostic> {
  let mut found = Vec::new();
  collect_invalid(rule_name, options, &mut found);
  found
}

/// The recursive walk behind [`invalid_options`].
fn collect_invalid(rule_name: &str, value: &serde_json::Value, found: &mut Vec<Diagnostic>) {
  match value {
    serde_json::Value::String(text) => check_literal(rule_name, text, found),
    serde_json::Value::Array(items) => {
      for item in items {
        collect_invalid(rule_name, item, found);
      }
    }
    serde_json::Value::Object(map) => {
      for (key, item) in map {
        if NON_PATTERN_OPTIONS.contains(&key.as_str()) {
          continue;
        }
        check_literal(rule_name, key, found);
        collect_invalid(rule_name, item, found);
      }
    }
    _ => {}
  }
}

/// Report `text` if it is a regex literal that does not compile.
fn check_literal(rule_name: &str, text: &str, found: &mut Vec<Diagnostic>) {
  if split_literal(text).is_some()
    && let Err(reason) = compile(text)
  {
    found.push(invalid_option(rule_name, text, &reason));
  }
}

/// Stylelint's report for an option value a rule cannot use, here a pattern
/// that does not compile: `Invalid option value "<value>" for rule "<rule>"`,
/// followed by the reason.
pub fn invalid_option(rule_name: &str, value: &str, reason: &str) -> Diagnostic {
  Diagnostic::invalid_option(format!(
    "Invalid option value \"{value}\" for rule \"{rule_name}\": {reason}"
  ))
}

/// Compile `pattern` without the cache.
fn build(pattern: &str) -> Result<Arc<Regex>, String> {
  let (source, flags) = split_literal(pattern).unwrap_or((pattern, ""));
  let inline: String = flags.chars().filter(|f| "ims".contains(*f)).collect();
  let full = if inline.is_empty() {
    source.to_string()
  } else {
    format!("(?{inline}){source}")
  };
  Regex::new(&full)
    .map(Arc::new)
    .map_err(|error| format!("not a valid regular expression ({error})"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn javascript_lookaround_compiles() {
    let re = compile("^(?!js-)[a-z-]+$").unwrap();
    assert!(is_match(&re, "card-title"));
    assert!(!is_match(&re, "js-card"));

    let behind = compile("(?<!-)bar").unwrap();
    assert!(is_match(&behind, "foobar"));
    assert!(!is_match(&behind, "foo-bar"));

    let backref = compile(r"^(a+)-\1$").unwrap();
    assert!(is_match(&backref, "aa-aa"));
    assert!(!is_match(&backref, "aa-a"));
  }

  #[test]
  fn regex_literals_apply_their_flags() {
    let re = compile("/^[a-z]+$/i").unwrap();
    assert!(is_match(&re, "ABC"));
    // `g` and `u` change nothing for a match test.
    assert!(is_match(&compile("/^a/gu").unwrap(), "abc"));
    // Without the literal form the slashes are part of the pattern.
    assert!(!is_match(&compile("^[a-z]+$").unwrap(), "ABC"));
  }

  #[test]
  fn split_literal_needs_a_source_and_javascript_flags() {
    assert_eq!(split_literal("/foo/"), Some(("foo", "")));
    assert_eq!(split_literal("/a/b/i"), Some(("a/b", "i")));
    assert_eq!(split_literal("/foo/x"), None);
    assert_eq!(split_literal("foo"), None);
    assert_eq!(split_literal("/"), None);
    assert_eq!(split_literal("//"), None);
  }

  #[test]
  fn list_entries_are_regexes_only_in_literal_form() {
    assert_eq!(match_regex_entry("/^-webkit-/i", "-WEBKIT-box"), Some(true));
    assert_eq!(match_regex_entry("/^(?!x)/", "x"), Some(false));
    assert_eq!(match_regex_entry("/[/", "anything"), Some(false));
    assert_eq!(match_regex_entry("color", "color"), None);
    assert!(regex_entry("/^a/").is_some());
    assert!(regex_entry("/[/").is_none());
    assert!(regex_entry("a").is_none());
  }

  #[test]
  fn entries_match_as_regex_literals_or_exact_strings() {
    assert!(matches_entry("/^font/", "font-size"));
    assert!(matches_entry("/^FONT/i", "font-size"));
    assert!(matches_entry("color", "color"));
    assert!(!matches_entry("color", "background-color"));
    assert!(!matches_entry("Color", "color"));

    let list = serde_json::json!(["include", "/^mix/"]);
    assert!(option_matches(Some(&list), "include"));
    assert!(option_matches(Some(&list), "mixin"));
    assert!(!option_matches(Some(&list), "media"));
    assert!(option_matches(Some(&serde_json::json!("media")), "media"));
    assert!(!option_matches(Some(&serde_json::json!(true)), "media"));
    assert!(!option_matches(None, "media"));
  }

  #[test]
  fn invalid_options_finds_broken_literals_anywhere_in_the_options() {
    let options = serde_json::json!([
      "always",
      {
        "ignoreProperties": ["/^ok/", "/[broken/", "plain[text"],
        "/(unclosed/": ["x"],
        "message": "/[not a pattern/"
      }
    ]);
    let found: Vec<String> = invalid_options("some-rule", &options)
      .into_iter()
      .map(|d| {
        assert!(d.is_invalid_option());
        d.message
      })
      .collect();
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
      found
        .iter()
        .any(|m| m.starts_with("Invalid option value \"/[broken/\" for rule \"some-rule\""))
    );
    assert!(
      found
        .iter()
        .any(|m| m.starts_with("Invalid option value \"/(unclosed/\""))
    );
    assert!(invalid_options("some-rule", &serde_json::json!(true)).is_empty());
  }

  #[test]
  fn compile_errors_are_cached_and_reported_as_invalid_options() {
    let first = compile("[a-z").unwrap_err();
    assert!(
      first.starts_with("not a valid regular expression"),
      "{first}"
    );
    assert_eq!(compile("[a-z").unwrap_err(), first);

    let diag = for_rule("selector-class-pattern", "[a-z").unwrap_err();
    assert!(diag.is_invalid_option());
    assert!(
      diag
        .message
        .starts_with("Invalid option value \"[a-z\" for rule \"selector-class-pattern\": "),
      "{}",
      diag.message
    );
  }

  #[test]
  fn the_cache_hands_back_the_same_compiled_pattern() {
    let a = compile("^cached-[0-9]+$").unwrap();
    let b = compile("^cached-[0-9]+$").unwrap();
    assert!(Arc::ptr_eq(&a, &b));
  }
}
