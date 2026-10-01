use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity};

use crate::file_cache::FileCache;
use crate::postcss_tree::PostcssTree;
use crate::source_text::{self, WrittenDeclaration};
use crate::style_rules::{self, ScannedRules};

/// Context passed to each rule when checking a node.
pub struct RuleContext<'a> {
  /// The path to the file being linted.
  pub file_path: &'a str,
  /// The full source text of the file.
  pub source: &'a str,
  /// The CSS syntax variant.
  pub syntax: Syntax,
  /// Per-rule options from the config (e.g. max value, ignore lists).
  pub options: Option<&'a serde_json::Value>,
  /// What the rules linting this file share, built once per file.  The
  /// runner always sets it; without one (a rule's own tests, say) every
  /// accessor such as [`Self::postcss_tree`] builds afresh.
  pub cache: Option<&'a FileCache<'a>>,
}

impl<'a> RuleContext<'a> {
  /// Extract the **primary option** from Stylelint-format options.
  ///
  /// Options may arrive in several shapes depending on how the config was
  /// written and resolved:
  ///
  /// - A plain value (string, number, bool): `"always"`, `4`, `true`
  /// - An array `[primary, secondary]`: `["always", { "except": [...] }]`
  ///
  /// This method normalises all forms and returns the primary option value.
  pub fn primary_option(&self) -> Option<&'a serde_json::Value> {
    let value = self.options?;
    match value {
      serde_json::Value::Array(arr) => arr.first(),
      other => Some(other),
    }
  }

  /// Extract the primary option as a `&str`, handling both plain strings
  /// and array-format options `["value", { ... }]`.
  pub fn primary_option_str(&self) -> Option<&'a str> {
    self.primary_option().and_then(|v| v.as_str())
  }

  /// Extract the **secondary options object** from Stylelint-format options.
  ///
  /// Present when options are in array form `[primary, secondary]` (returns
  /// the second element), OR when options is a bare object (returned directly,
  /// e.g. from preset configs that store `{"ignore": [...]}` without a
  /// primary option wrapper).
  pub fn secondary_options(&self) -> Option<&'a serde_json::Value> {
    secondary_options_of(self.options?)
  }

  /// `source[start..end]`, or `None` when the range is out of bounds,
  /// reversed, or splits a multibyte character.
  ///
  /// Offsets a rule computes (a node span plus the length of some AST text,
  /// say) do not always line up with the source the author wrote, and a
  /// plain slice panics the moment one lands inside `é` or `😀`.  Slice
  /// through this instead and decide what a miss means for the rule.
  pub fn source_slice(&self, start: usize, end: usize) -> Option<&'a str> {
    self.source.get(start..end)
  }

  /// `source[start..]`, or `None` when `start` is past the end or inside a
  /// multibyte character.  See [`Self::source_slice`].
  pub fn source_from(&self, start: usize) -> Option<&'a str> {
    self.source.get(start..)
  }

  /// The selector of the style rule that starts at `offset`, exactly as
  /// written: everything up to the `{` that opens its block, with trailing
  /// whitespace removed.
  ///
  /// The parsed selector is no substitute when positions matter: the CSS
  /// parser re-serialises it, so its length need not match the source.
  /// Returns `None` when `offset` is not a character boundary in the source.
  pub fn selector_source(&self, offset: usize) -> Option<&'a str> {
    let rest = self.source_from(offset)?;
    let end = selector_end(rest, !matches!(self.syntax, Syntax::Css));
    rest.get(..end).map(str::trim_end)
  }

  /// The file's [`FileCache`], when it was built for this context's source
  /// and syntax.
  fn cache(&self) -> Option<&'a FileCache<'a>> {
    self
      .cache
      .filter(|cache| cache.is_for(self.source, self.syntax))
  }

  /// The source's statements as PostCSS sees them, parsed once per file and
  /// shared with every other rule that asks.
  pub fn postcss_tree(&self) -> Rc<PostcssTree<'a>> {
    match self.cache() {
      Some(cache) => cache.postcss_tree(),
      None => Rc::new(PostcssTree::parse(self.source, self.syntax)),
    }
  }

  /// The source's style rule preludes and at-rules as written, scanned once
  /// per file and shared with every other rule that asks.
  pub fn scanned_rules(&self) -> Rc<ScannedRules> {
    match self.cache() {
      Some(cache) => cache.scanned_rules(),
      None => Rc::new(style_rules::scan(self.source, self.syntax)),
    }
  }

  /// The declarations `node` holds directly, as written (see
  /// [`source_text::written_declarations`]).  Every rule checking the node
  /// the runner is on shares one reading of them.
  pub fn written_declarations(&self, node: &CssNode) -> Rc<Vec<WrittenDeclaration<'a>>> {
    match self.cache() {
      Some(cache) => cache.written_declarations(node),
      None => Rc::new(source_text::written_declarations(
        self.source,
        node,
        self.syntax,
      )),
    }
  }
}

/// Byte length of the selector at the start of `text`: the offset of the
/// first `{` that is not inside a string, comment, attribute selector,
/// parentheses or `#{...}` interpolation, or the whole text if there is none.
///
/// `line_comments` treats `//` as a comment to the end of the line, as in
/// SCSS and Less.
fn selector_end(text: &str, line_comments: bool) -> usize {
  let bytes = text.as_bytes();
  let mut depth = 0usize;
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'{' if depth == 0 => return i,
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
          .map_or(bytes.len(), |end| i + 2 + end + 1);
      }
      b'/' if line_comments && bytes.get(i + 1) == Some(&b'/') => {
        i = text[i..].find('\n').map_or(bytes.len(), |end| i + end);
      }
      b'#' if bytes.get(i + 1) == Some(&b'{') => {
        depth += 1;
        i += 1;
      }
      b'(' | b'[' => depth += 1,
      b')' | b']' | b'}' => depth = depth.saturating_sub(1),
      _ => {}
    }
    i += 1;
  }
  bytes.len()
}

/// The secondary options object inside a Stylelint-format rule setting.
///
/// Array form `[primary, secondary]` yields the second element; a bare object
/// (as preset configs store `{"ignore": [...]}`) is returned as-is.
pub fn secondary_options_of(value: &serde_json::Value) -> Option<&serde_json::Value> {
  match value {
    serde_json::Value::Array(arr) => arr.get(1),
    serde_json::Value::Object(_) => Some(value),
    _ => None,
  }
}

/// A single lint rule that can inspect CSS AST nodes and emit diagnostics.
pub trait Rule: Send + Sync {
  /// Unique rule name, e.g. "block-no-empty".
  fn name(&self) -> &'static str;

  /// Human-readable description of what this rule checks.
  fn description(&self) -> &'static str;

  /// The default severity for diagnostics produced by this rule.
  fn default_severity(&self) -> Severity;

  /// Check a single CSS node and return any diagnostics.
  fn check(&self, node: &CssNode, context: &RuleContext) -> Vec<Diagnostic> {
    let _ = (node, context);
    vec![]
  }

  /// Check the entire document (all top-level nodes). Used for rules that need
  /// document-level context like duplicate detection or source-level checks.
  fn check_root(&self, _nodes: &[CssNode], _context: &RuleContext) -> Vec<Diagnostic> {
    vec![]
  }
}

/// How many parsed options values [`per_run`] keeps for each rule and
/// parsed type on a thread.  A run needs one for each config the rule is
/// set in; the language server, which outlives configs, keeps the latest.
const PARSED_OPTIONS_PER_RULE: usize = 8;

/// One options value [`per_run`] parsed, and what the parse returned.
struct ParsedOptions {
  /// A copy of the options value, compared by content.
  options: Option<serde_json::Value>,
  /// What `parse` returned.
  parsed: Rc<dyn Any>,
}

thread_local! {
  /// Options parsed on this thread, by rule and parsed type, the most
  /// recent last.
  static PARSED_OPTIONS: RefCell<HashMap<(&'static str, TypeId), Vec<ParsedOptions>>> =
    RefCell::new(HashMap::new());
}

/// Parse a rule's options at most once per run.
///
/// A rule whose options take real work to interpret (long lists, lookup
/// tables) wraps that work in this, and it is done once on each worker
/// thread for each distinct options value, however many files the run
/// lints.  `parse` must depend only on `options`.
///
/// Options are compared by content rather than by address, so the copy a
/// config override makes for every file, or a config the language server
/// reloads, still finds its parse, and an address that a dropped value
/// left behind never finds another value's.
pub fn per_run<T: 'static>(
  rule: &'static str,
  options: Option<&serde_json::Value>,
  parse: impl FnOnce() -> T,
) -> Rc<T> {
  let key = (rule, TypeId::of::<T>());
  let cached = PARSED_OPTIONS.with(|memo| {
    let memo = memo.borrow();
    let entry = memo
      .get(&key)?
      .iter()
      .rev()
      .find(|entry| entry.options.as_ref() == options)?;
    Rc::clone(&entry.parsed).downcast::<T>().ok()
  });
  if let Some(parsed) = cached {
    return parsed;
  }
  // Parse without holding the memo borrowed, so a panicking parse leaves
  // it unlocked and caches nothing.
  let parsed = Rc::new(parse());
  PARSED_OPTIONS.with(|memo| {
    let mut memo = memo.borrow_mut();
    let entries = memo.entry(key).or_default();
    if entries.len() >= PARSED_OPTIONS_PER_RULE {
      entries.remove(0);
    }
    entries.push(ParsedOptions {
      options: options.cloned(),
      parsed: Rc::clone(&parsed) as Rc<dyn Any>,
    });
  });
  parsed
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::cell::Cell;

  /// A context over `source` with no options.
  fn context(source: &str, syntax: Syntax) -> RuleContext<'_> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax,
      options: None,
      cache: None,
    }
  }

  #[test]
  fn source_slice_refuses_ranges_that_split_a_character() {
    let ctx = context("a{b:é}", Syntax::Css);
    assert_eq!(ctx.source_slice(4, 6), Some("é"));
    assert_eq!(ctx.source_slice(4, 5), None, "inside é");
    assert_eq!(ctx.source_slice(5, 7), None, "starts inside é");
    assert_eq!(ctx.source_slice(3, 99), None, "past the end");
    assert_eq!(ctx.source_slice(5, 4), None, "reversed");
    assert_eq!(ctx.source_from(6), Some("}"));
    assert_eq!(ctx.source_from(5), None);
  }

  #[test]
  fn selector_source_reads_up_to_the_opening_brace() {
    let ctx = context("a ,\n  b中 { color: red; }", Syntax::Css);
    assert_eq!(ctx.selector_source(0), Some("a ,\n  b中"));
    assert_eq!(ctx.selector_source(8), None, "inside 中");
  }

  #[test]
  fn selector_source_skips_braces_that_do_not_open_the_block() {
    let scss = context(
      ".a-#{$b}, [x=\"{\"] /* { */ .c(d) // {\n.e { }",
      Syntax::Scss,
    );
    assert_eq!(
      scss.selector_source(0),
      Some(".a-#{$b}, [x=\"{\"] /* { */ .c(d) // {\n.e")
    );
    // Plain CSS has no line comments, so `//` is just text.
    let css = context("a // b { }", Syntax::Css);
    assert_eq!(css.selector_source(0), Some("a // b"));
  }

  /// A context with the runner's per-file cache shares its artifacts
  /// between rules; one over other text, or without a cache, builds its own.
  #[test]
  fn artifacts_come_from_the_cache_built_for_the_source() {
    let source = String::from("a { color: red; }");
    let cache = FileCache::new(&source, Syntax::Css);
    let ctx = RuleContext {
      cache: Some(&cache),
      ..context(&source, Syntax::Css)
    };
    assert!(Rc::ptr_eq(&ctx.postcss_tree(), &cache.postcss_tree()));
    assert!(Rc::ptr_eq(&ctx.scanned_rules(), &cache.scanned_rules()));

    let other = source.clone();
    let elsewhere = RuleContext {
      cache: Some(&cache),
      ..context(&other, Syntax::Css)
    };
    let tree = elsewhere.postcss_tree();
    assert!(!Rc::ptr_eq(&tree, &cache.postcss_tree()));
    assert!(std::ptr::eq(tree.source(), other.as_str()));
    assert!(!Rc::ptr_eq(
      &elsewhere.scanned_rules(),
      &cache.scanned_rules()
    ));

    let uncached = context(&source, Syntax::Css);
    assert!(!Rc::ptr_eq(
      &uncached.postcss_tree(),
      &uncached.postcss_tree()
    ));
  }

  /// Calls `per_run` for `options` as `rule`, counting how often it had to
  /// parse.
  fn parse_counting(
    rule: &'static str,
    options: &serde_json::Value,
    parses: &Cell<usize>,
  ) -> Rc<usize> {
    per_run(rule, Some(options), || {
      parses.set(parses.get() + 1);
      options.as_array().map_or(0, Vec::len)
    })
  }

  /// Each distinct options value is parsed once, whichever copy of it a
  /// file hands over, and the parse is kept from one file to the next.
  #[test]
  fn per_run_parses_each_options_value_once() {
    let first = serde_json::json!([1, 2, 3]);
    let second = serde_json::json!([1]);
    let parses = Cell::new(0);
    for _ in 0..5 {
      // A fresh copy each time, as config overrides make for every file.
      let copy = first.clone();
      assert_eq!(*parse_counting("test/once", &copy, &parses), 3);
      assert_eq!(*parse_counting("test/once", &second, &parses), 1);
    }
    assert_eq!(parses.get(), 2);

    // Another rule, or another parsed type, gets its own entry.
    assert_eq!(*per_run("test/other", Some(&first), || 7usize), 7);
    let as_string = per_run("test/once", Some(&first), || "parsed".to_string());
    assert_eq!(*as_string, "parsed");
    assert_eq!(*per_run("test/once", None, || 0usize), 0);
    assert_eq!(
      *per_run("test/once", None, || 1usize),
      0,
      "no options is a value too"
    );
  }

  /// A parse that panics caches nothing, and the next call parses again.
  #[test]
  fn per_run_keeps_nothing_from_a_parse_that_panics() {
    let options = serde_json::json!(["a"]);
    let caught = crate::panic_guard::catch(|| {
      per_run("test/panics", Some(&options), || -> usize {
        panic!("bad options")
      })
    })
    .unwrap_err();
    assert_eq!(caught.message, "bad options");
    assert_eq!(*per_run("test/panics", Some(&options), || 1usize), 1);
    assert_eq!(*per_run("test/panics", Some(&options), || 2usize), 1);
  }

  /// Only the latest few options values of a rule are kept.
  #[test]
  fn per_run_keeps_the_latest_options_values() {
    let parses = Cell::new(0);
    let values: Vec<serde_json::Value> = (0..=PARSED_OPTIONS_PER_RULE)
      .map(|n| serde_json::json!(vec![0; n]))
      .collect();
    for value in &values {
      parse_counting("test/latest", value, &parses);
    }
    assert_eq!(parses.get(), values.len());
    // The newest are still there; the oldest was dropped.
    parse_counting("test/latest", &values[values.len() - 1], &parses);
    assert_eq!(parses.get(), values.len());
    parse_counting("test/latest", &values[0], &parses);
    assert_eq!(parses.get(), values.len() + 1);
  }
}
