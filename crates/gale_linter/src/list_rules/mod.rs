//! The engine behind the `*-allowed-list` and `*-disallowed-list` rules.
//!
//! Every such rule does the same thing: find the things of one kind in the
//! stylesheet (at-rule names, properties, functions, units, pseudo-classes,
//! URL schemes, ...), match each against the configured list, and report
//! the ones the list rejects: those it does not hold for an allowed list,
//! those it does hold for a disallowed one.  A rule file says only what
//! differs, by implementing [`ListRule`]: which subjects it looks at (the
//! finders in [`subjects`]), its [`Polarity`], how a subject is normalised
//! before matching, how a problem is worded, and any secondary options.
//! [`ListCheck`] reads the list once and does the rest.
//!
//! The `declaration-property-*-list`, `media-feature-name-*-allowed-list`
//! and `rule-selector-property-disallowed-list` rules take an object
//! instead, mapping a key (a property, a media feature name, a selector) to
//! a list.  They implement [`KeyedListRule`] and run on [`KeyedCheck`].
//!
//! Matching is Stylelint's `matchesStringOrRegExp`: a name must equal the
//! subject exactly, and a `/regex/` entry (with any flags) need only match
//! part of it.  Stylelint 17 matches most subjects exactly as written; a
//! rule that lowercases one first (the units) says so in
//! [`ListRule::normalize`] or [`KeyedListRule::normalize`].  Lists whose
//! entries Stylelint takes as plain strings only (`[isString]`: units,
//! combinators, attribute operators) compare every entry literally.

pub mod subjects;

use std::borrow::Cow;
use std::marker::PhantomData;
use std::sync::Arc;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};
use serde_json::{Map, Value};

use crate::lint_env::LintEnv;
use crate::options::{Accept, InvalidOption, Options, Setting};
use crate::pattern::{self, Regex};
use crate::postcss_tree::PostcssTree;
use crate::rule::{Check, RuleContext};

/// Whether a list names what is allowed or what is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
  /// Report every subject the list does not match.
  Allowed,
  /// Report every subject the list matches.
  Disallowed,
}

impl Polarity {
  /// Whether a subject the list does (`listed`) or does not match is
  /// reported.
  fn reports(self, listed: bool) -> bool {
    listed == (self == Polarity::Disallowed)
  }
}

/// How a list's entries are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entries {
  /// Names and `/regex/` literals: Stylelint's `[isString, isRegExp]`.
  Patterns,
  /// Plain strings only (`[isString]`), compared literally, as a rule that
  /// tests `list.includes(subject)` does.
  Names,
}

/// One entry of a list: a name, or a `/regex/` literal.
#[derive(Debug, Clone)]
struct Entry {
  /// The entry as written.
  text: String,
  /// The compiled expression of a `/regex/` entry: `Some(None)` for one
  /// that does not compile, which matches nothing (the rule set reports it
  /// as an invalid option).
  regex: Option<Option<Arc<Regex>>>,
}

impl Entry {
  /// `text` read as `entries` says.
  fn new(text: &str, entries: Entries) -> Self {
    let regex = (entries == Entries::Patterns && pattern::split_literal(text).is_some())
      .then(|| pattern::compile(text).ok());
    Self {
      text: text.to_string(),
      regex,
    }
  }

  /// Whether the entry matches `subject`: a name by equality, a regex by
  /// search.
  fn matches(&self, subject: &str) -> bool {
    match &self.regex {
      None => self.text == subject,
      Some(Some(regex)) => pattern::is_match(regex, subject),
      Some(None) => false,
    }
  }
}

/// A list option, read once: the entries a rule matches its subjects
/// against.
#[derive(Debug, Clone, Default)]
pub struct List(Vec<Entry>);

impl List {
  /// The list of `entries`, read as `kind` says.
  pub fn new<S: AsRef<str>>(entries: &[S], kind: Entries) -> Self {
    Self(
      entries
        .iter()
        .map(|entry| Entry::new(entry.as_ref(), kind))
        .collect(),
    )
  }

  /// The first entry that matches `subject` (Stylelint's
  /// `matchesStringOrRegExp`), as written.
  pub fn find(&self, subject: &str) -> Option<&str> {
    self
      .0
      .iter()
      .find(|entry| entry.matches(subject))
      .map(|entry| entry.text.as_str())
  }

  /// Whether any entry matches `subject`.
  pub fn matches(&self, subject: &str) -> bool {
    self.find(subject).is_some()
  }

  /// The first plain entry that `subject` contains (Stylelint's
  /// `containsString`, which skips `/regex/` entries).
  pub fn find_within(&self, subject: &str) -> Option<&str> {
    self
      .0
      .iter()
      .find(|entry| {
        entry.regex.is_none() && !entry.text.is_empty() && subject.contains(&entry.text)
      })
      .map(|entry| entry.text.as_str())
  }

  /// The part of `subject` that `entry` (one of this list's) matches, and
  /// where it starts: the first occurrence of a name, what a regex finds.
  pub fn matched_text<'s>(&self, entry: &str, subject: &'s str) -> Option<(usize, &'s str)> {
    let entry = self.0.iter().find(|candidate| candidate.text == entry)?;
    match &entry.regex {
      None => subject
        .find(entry.text.as_str())
        .map(|at| (at, &subject[at..at + entry.text.len()])),
      Some(Some(regex)) => regex
        .find(subject)
        .ok()
        .flatten()
        .map(|found| (found.start(), found.as_str())),
      Some(None) => None,
    }
  }
}

/// Stylelint's `validateObjectWithArrayProps(isString, isRegExp)`: an
/// object whose every value is a string or a list of strings.
fn is_string_list_object(value: &Value) -> bool {
  value
    .as_object()
    .is_some_and(|map| map.values().all(|item| strings_of(item).is_some()))
}

/// The strings of a string or an array of strings; `None` when anything
/// else is in it.
fn strings_of(value: &Value) -> Option<Vec<&str>> {
  match value {
    Value::String(text) => Some(vec![text]),
    Value::Array(items) => items.iter().map(Value::as_str).collect(),
    _ => None,
  }
}

/// One thing a list rule found in the stylesheet.
#[derive(Debug, Clone)]
pub struct Found {
  /// The subject as written: what the list is matched against (after
  /// [`ListRule::normalize`]) and what the message quotes.
  pub name: String,
  /// Where the problem is reported.
  pub span: Span,
  /// The statement the subject is in: an index into the [`PostcssTree`].
  pub node: usize,
  /// More of the statement for the message to quote, such as the path an
  /// extension belongs to; usually empty.
  pub detail: String,
}

impl Found {
  /// `name`, found in statement `node` at `start..end`.
  pub fn new(name: impl Into<String>, node: usize, start: usize, end: usize) -> Self {
    Self {
      name: name.into(),
      span: Span::from_range(start, end.max(start)),
      node,
      detail: String::new(),
    }
  }

  /// The subject with `detail` for the message.
  pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
    self.detail = detail.into();
    self
  }
}

/// A rule whose primary option is a list of names and `/regex/` literals.
///
/// The unit struct a rule file declares with `declare_rule!` implements
/// this, and names `ListCheck<Self>` as its configured form.
pub trait ListRule: Send + Sync + 'static {
  /// The rule's name.
  const NAME: &'static str;
  /// Whether the list says what is allowed or what is not.
  const POLARITY: Polarity;
  /// How the list's entries are read.
  const ENTRIES: Entries = Entries::Patterns;

  /// The rule's secondary options, read once.
  type Options: Default + Send + Sync;

  /// Read the rule's secondary options.  A rule that takes none reads
  /// nothing, so whatever secondary options it is given are ignored, as
  /// Stylelint does for a rule that validates none.
  fn options(options: &mut Options<'_>) -> Self::Options {
    let _ = options;
    Self::Options::default()
  }

  /// Every subject in the stylesheet, in document order.
  fn find(tree: &PostcssTree, ctx: &RuleContext, options: &Self::Options, found: &mut Vec<Found>);

  /// The subject as it is matched against the list.
  fn normalize(name: &str) -> Cow<'_, str> {
    Cow::Borrowed(name)
  }

  /// The entry of `list` that matches the subject, if any.
  fn listed<'l>(
    list: &'l List,
    options: &Self::Options,
    found: &Found,
    tree: &PostcssTree,
  ) -> Option<&'l str> {
    let _ = (options, tree);
    list.find(&Self::normalize(&found.name))
  }

  /// The problem's message.  `entry` is the list entry that matched, for a
  /// disallowed list.
  fn message(found: &Found, entry: Option<&str>) -> String;

  /// Where the problem is reported, when that depends on what matched.
  fn span(list: &List, found: &Found, entry: Option<&str>) -> Span {
    let _ = (list, entry);
    found.span
  }
}

/// A [`ListRule`] configured with its list.
pub struct ListCheck<R: ListRule> {
  /// The list, or `None` when the config gives none (`true`, or a preset
  /// enabling the rule), in which case the rule reports nothing.
  list: Option<List>,
  options: R::Options,
  rule: PhantomData<fn() -> R>,
}

impl<R: ListRule> ListCheck<R> {
  /// Read the list (a name, a `/regex/`, or a list of them) and the
  /// rule's secondary options.
  pub fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let list = options
      .primary_strings()
      .map(|entries| List::new(&entries, R::ENTRIES));
    let extra = R::options(&mut options);
    options.finish()?;
    Ok(Self {
      list,
      options: extra,
      rule: PhantomData,
    })
  }
}

impl<R: ListRule> Check for ListCheck<R> {
  /// Report every subject the list rejects.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let Some(list) = &self.list else {
      return Vec::new();
    };
    let tree = ctx.postcss_tree();
    let mut found = Vec::new();
    R::find(&tree, ctx, &self.options, &mut found);
    found
      .iter()
      .filter_map(|subject| {
        let entry = R::listed(list, &self.options, subject, &tree);
        R::POLARITY.reports(entry.is_some()).then(|| {
          Diagnostic::new(R::NAME, R::message(subject, entry)).span(R::span(list, subject, entry))
        })
      })
      .collect()
  }
}

/// An object option mapping keys (names or `/regex/` literals) to lists.
#[derive(Debug, Clone, Default)]
pub struct KeyedList(Vec<(Entry, List)>);

impl KeyedList {
  /// Read the primary option as Stylelint's
  /// `validateObjectWithArrayProps(...)` checks it: an object whose every
  /// value is a string or a list of strings.  Read it before any secondary
  /// option.
  pub fn read(options: &mut Options<'_>, rule: &str, values: Entries) -> Option<Self> {
    let map: &Map<String, Value> = options.primary_object()?;
    if !is_string_list_object(&Value::Object(map.clone())) {
      options.report(InvalidOption::value(rule, &Value::Object(map.clone())));
      return None;
    }
    Some(Self(
      map
        .iter()
        .map(|(key, list)| {
          let entries = strings_of(list).unwrap_or_default();
          (
            Entry::new(key, Entries::Patterns),
            List::new(&entries, values),
          )
        })
        .collect(),
    ))
  }

  /// The list of the first key, in the order the config wrote them, that
  /// matches `key`.
  pub fn first(&self, key: &str) -> Option<&List> {
    self
      .0
      .iter()
      .find(|(entry, _)| entry.matches(key))
      .map(|(_, list)| list)
  }

  /// The lists of every key that matches `key`.
  pub fn matching<'k>(&'k self, key: &'k str) -> impl Iterator<Item = &'k List> + 'k {
    self
      .0
      .iter()
      .filter(move |(entry, _)| entry.matches(key))
      .map(|(_, list)| list)
  }
}

/// One thing a keyed list rule found: the key that picks the list, and the
/// subject matched against it.
#[derive(Debug, Clone)]
pub struct KeyedFound {
  /// What the keys are matched against: a property, a media feature name
  /// or a selector, as written.
  pub key: String,
  /// The subject matched against the key's list (after
  /// [`KeyedListRule::normalize`]): a unit, a value or a property.
  pub name: String,
  /// Where the problem is reported.
  pub span: Span,
}

impl KeyedFound {
  /// `name` under `key`, found at `start..end`.
  pub fn new(key: impl Into<String>, name: impl Into<String>, start: usize, end: usize) -> Self {
    Self {
      key: key.into(),
      name: name.into(),
      span: Span::from_range(start, end.max(start)),
    }
  }
}

/// A rule whose primary option maps keys to lists.
pub trait KeyedListRule: Send + Sync + 'static {
  /// The rule's name.
  const NAME: &'static str;
  /// Whether the lists say what is allowed or what is not.
  const POLARITY: Polarity;
  /// How the lists' entries are read.
  const ENTRIES: Entries = Entries::Patterns;
  /// Whether every key that matches applies (Stylelint's `filter`), or
  /// only the first (`find`).
  const EVERY_KEY: bool = false;

  /// The rule's secondary options, read once.
  type Options: Default + Send + Sync;

  /// Read the rule's secondary options; none by default.
  fn options(options: &mut Options<'_>) -> Self::Options {
    let _ = options;
    Self::Options::default()
  }

  /// Every subject in the stylesheet, in document order.
  fn find(
    tree: &PostcssTree,
    ctx: &RuleContext,
    options: &Self::Options,
    found: &mut Vec<KeyedFound>,
  );

  /// The subject as it is matched against a list.
  fn normalize(name: &str) -> Cow<'_, str> {
    Cow::Borrowed(name)
  }

  /// The problem's message.
  fn message(found: &KeyedFound) -> String;
}

/// A [`KeyedListRule`] configured with its lists.
pub struct KeyedCheck<R: KeyedListRule> {
  lists: Option<KeyedList>,
  options: R::Options,
  rule: PhantomData<fn() -> R>,
}

impl<R: KeyedListRule> KeyedCheck<R> {
  /// Read the object of lists and the rule's secondary options.
  pub fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let lists = KeyedList::read(&mut options, R::NAME, R::ENTRIES);
    let extra = R::options(&mut options);
    options.finish()?;
    Ok(Self {
      lists,
      options: extra,
      rule: PhantomData,
    })
  }

  /// Whether the lists that apply to `found` reject it; `false` when no
  /// key matches.
  fn rejects(&self, lists: &KeyedList, found: &KeyedFound) -> bool {
    let subject = R::normalize(&found.name);
    if R::EVERY_KEY {
      let mut applying = lists.matching(&found.key).peekable();
      if applying.peek().is_none() {
        return false;
      }
      let listed = match R::POLARITY {
        Polarity::Allowed => applying.any(|list| list.matches(&subject)),
        Polarity::Disallowed => !applying.all(|list| !list.matches(&subject)),
      };
      R::POLARITY.reports(listed)
    } else {
      lists
        .first(&found.key)
        .is_some_and(|list| R::POLARITY.reports(list.matches(&subject)))
    }
  }
}

impl<R: KeyedListRule> Check for KeyedCheck<R> {
  /// Report every subject the list for its key rejects.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let Some(lists) = &self.lists else {
      return Vec::new();
    };
    let tree = ctx.postcss_tree();
    let mut found = Vec::new();
    R::find(&tree, ctx, &self.options, &mut found);
    found
      .iter()
      .filter(|subject| self.rejects(lists, subject))
      .map(|subject| Diagnostic::new(R::NAME, R::message(subject)).span(subject.span))
      .collect()
  }
}

/// A secondary option mapping names (units, say) to lists, as Stylelint's
/// `validateObjectWithArrayProps(isString, isRegExp)` checks it.
pub fn keyed_option(options: &mut Options<'_>, key: &'static str) -> KeyedList {
  let Some(Value::Object(map)) = options.value(key, &[Accept::Test(is_string_list_object)]) else {
    return KeyedList::default();
  };
  KeyedList(
    map
      .iter()
      .map(|(name, list)| {
        let entries = strings_of(list).unwrap_or_default();
        (
          Entry::new(name, Entries::Names),
          List::new(&entries, Entries::Patterns),
        )
      })
      .collect(),
  )
}

/// A secondary option listing names and `/regex/` literals.
pub fn list_option(options: &mut Options<'_>, key: &'static str) -> List {
  List::new(&options.strings(key), Entries::Patterns)
}

#[cfg(test)]
pub mod testing {
  //! Table-driven tests for list rules: lint a snippet with one rule and
  //! compare the warnings as `line:column message`.

  use gale_css_parser::Syntax;
  use serde_json::Value;

  /// The warnings `rule` configured with `options` reports for `source`,
  /// as `line:column message`.
  pub fn warnings(rule: &str, options: Value, source: &str, syntax: Syntax) -> Vec<String> {
    crate::testing::lint(rule, options, source, syntax)
      .iter()
      .map(|warning| {
        let before = &source[..warning.span.offset.min(source.len())];
        let line = before.matches('\n').count() + 1;
        let column = before.len() - before.rfind('\n').map_or(0, |n| n + 1) + 1;
        format!("{line}:{column} {}", warning.message)
      })
      .collect()
  }

  /// Check every `(options, source, expected)` case of `rule` in CSS.
  pub fn check_css(rule: &str, cases: &[(Value, &str, &[&str])]) {
    check(rule, Syntax::Css, cases);
  }

  /// Check every `(options, source, expected)` case of `rule` in `syntax`.
  pub fn check(rule: &str, syntax: Syntax, cases: &[(Value, &str, &[&str])]) {
    for (options, source, expected) in cases {
      assert_eq!(
        warnings(rule, options.clone(), source, syntax),
        *expected,
        "{rule} {options} on {source:?}"
      );
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn names_match_exactly_and_regexes_by_search() {
    let list = List::new(&["color", "/^border/i", "/[broken/"], Entries::Patterns);
    assert_eq!(list.find("color"), Some("color"));
    assert_eq!(list.find("Color"), None);
    assert_eq!(list.find("BORDER-top"), Some("/^border/i"));
    assert_eq!(list.find("[broken"), None);
  }

  #[test]
  fn names_only_lists_compare_regex_literals_literally() {
    let list = List::new(&["/px/"], Entries::Names);
    assert!(list.matches("/px/"));
    assert!(!list.matches("px"));
  }

  #[test]
  fn find_within_skips_regexes() {
    let list = List::new(&["/todo/", "fixme"], Entries::Patterns);
    assert_eq!(list.find_within("a todo"), None);
    assert_eq!(list.find_within("a fixme here"), Some("fixme"));
    assert_eq!(list.matched_text("/todo/", "a todo"), Some((2, "todo")));
    assert_eq!(list.matched_text("fixme", "a fixme"), Some((2, "fixme")));
  }

  /// A rule whose primary option is an object, as the keyed list rules'.
  const OBJECT_RULE: crate::rule::RuleMeta = crate::rule::RuleMeta {
    name: "test-rule",
    description: "",
    default_severity: gale_diagnostics::Severity::Warning,
    primary_option_array: false,
    primary_option_object: true,
  };

  #[test]
  fn keyed_lists_pick_the_first_or_every_matching_key() {
    let options = serde_json::json!({ "/^margin/": ["px"], "margin-top": ["em"] });
    let setting = Setting::new(&OBJECT_RULE, Some(&options));
    let mut reader = setting.options();
    let lists = KeyedList::read(&mut reader, "test-rule", Entries::Names).unwrap();
    assert!(reader.finish().is_ok());
    assert!(lists.first("margin-top").unwrap().matches("px"));
    assert_eq!(lists.matching("margin-top").count(), 2);
    assert!(lists.first("padding").is_none());

    // The first key in the order written, as Stylelint's `find` takes it.
    let options = serde_json::json!({ "margin-top": ["em"], "/^margin/": ["px"] });
    let setting = Setting::new(&OBJECT_RULE, Some(&options));
    let mut reader = setting.options();
    let lists = KeyedList::read(&mut reader, "test-rule", Entries::Names).unwrap();
    assert!(lists.first("margin-top").unwrap().matches("em"));
    assert!(!lists.first("margin-top").unwrap().matches("px"));
  }

  #[test]
  fn keyed_lists_must_hold_strings() {
    let options = serde_json::json!({ "margin": [1] });
    let setting = Setting::new(&OBJECT_RULE, Some(&options));
    let mut reader = setting.options();
    assert!(KeyedList::read(&mut reader, "test-rule", Entries::Names).is_none());
    let problems: Vec<String> = reader
      .finish()
      .unwrap_err()
      .into_iter()
      .map(InvalidOption::into_text)
      .collect();
    assert_eq!(
      problems,
      ["Invalid option value \"{\"margin\":[1]}\" for rule \"test-rule\""]
    );
  }
}
