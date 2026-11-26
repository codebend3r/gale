//! Rule options, read once when a rule set is built.
//!
//! A config's setting for a rule is normalised into a [`Setting`] the way
//! Stylelint's `normalizeRuleSettings` does it: a primary option plus an
//! optional secondary options object.  A rule reads its setting through an
//! [`Options`] reader, which validates every value as Stylelint's
//! `validateOptions` does and words each problem exactly as Stylelint does:
//!
//! - `Invalid option value "${value}" for rule "${rule}"`
//! - `Invalid option name "${name}" for rule "${rule}"`
//! - `Invalid value "${value}" for option "${option}" of rule "${rule}"`
//!
//! A rule with any invalid option does not run, and every file linted with
//! the rule set carries the problems as invalid-option warnings.
//!
//! ```ignore
//! fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
//!   let mut options = setting.options();
//!   let expectation = options.primary_one_of(&["always", "never"]).unwrap_or("always");
//!   let ignore_properties = options.matchers("ignoreProperties");
//!   options.finish()?;
//!   Ok(Self { expectation, ignore_properties })
//! }
//! ```

use std::sync::Arc;

use serde_json::{Map, Value};

use crate::js_number;
use crate::pattern::{self, Regex};
use crate::rule::RuleMeta;

/// The secondary options every rule accepts, whatever the rule itself takes.
/// The runner applies them (`severity`, `message`, `url`, `disableFix`) or
/// the disable-comment reports read them (`reportDisables`), so a rule never
/// reads them and they are never unknown: Stylelint's `IGNORED_OPTIONS`.
pub const FRAMEWORK_KEYS: &[&str] = &[
  SEVERITY_KEY,
  MESSAGE_KEY,
  URL_KEY,
  REPORT_DISABLES_KEY,
  DISABLE_FIX_KEY,
];

/// `severity`, the secondary option that overrides a rule's severity.
pub const SEVERITY_KEY: &str = "severity";
/// `message`, the secondary option that replaces a rule's warning text.
pub const MESSAGE_KEY: &str = "message";
/// `url`, the secondary option that attaches a documentation link.
pub const URL_KEY: &str = "url";
/// `reportDisables`, the secondary option that reports every comment
/// disabling the rule.
pub const REPORT_DISABLES_KEY: &str = "reportDisables";
/// `disableFix`, the secondary option that drops a rule's autofix.
pub const DISABLE_FIX_KEY: &str = "disableFix";

/// One problem with a rule's options, as Stylelint words it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidOption(String);

impl InvalidOption {
  /// A problem worded as `text`.
  pub fn new(text: impl Into<String>) -> Self {
    Self(text.into())
  }

  /// `Invalid option value "<value>" for rule "<rule>"`: a primary option
  /// value (or one entry of a primary list) that the rule does not accept.
  pub fn value(rule: &str, value: &Value) -> Self {
    Self(format!(
      "Invalid option value {} for rule \"{rule}\"",
      stringify(value)
    ))
  }

  /// `Invalid option "<value>" for rule "<rule>"`: a primary option that
  /// fails a rule's test, when Stylelint's `possible` is a function rather
  /// than a list.
  pub fn rejected(rule: &str, value: &Value) -> Self {
    Self(format!(
      "Invalid option {} for rule \"{rule}\"",
      stringify(value)
    ))
  }

  /// `Unexpected option value "<value>" for rule "<rule>"`: a primary
  /// option given to a rule that takes none but `true`.
  pub fn unexpected(rule: &str, value: &Value) -> Self {
    Self(format!(
      "Unexpected option value {} for rule \"{rule}\"",
      stringify(value)
    ))
  }

  /// `Invalid option value "<value>" for rule "<rule>": should be an
  /// object`: secondary options that are not an object.
  pub fn not_an_object(rule: &str, value: &Value) -> Self {
    Self(format!(
      "Invalid option value {} for rule \"{rule}\": should be an object",
      stringify(value)
    ))
  }

  /// `Invalid option name "<name>" for rule "<rule>"`: a secondary option
  /// the rule does not know.
  pub fn name(rule: &str, name: &str) -> Self {
    Self(format!(
      "Invalid option name \"{name}\" for rule \"{rule}\""
    ))
  }

  /// `Invalid value "<value>" for option "<option>" of rule "<rule>"`: a
  /// secondary option value (or one entry of a list) the rule does not
  /// accept.
  pub fn option_value(rule: &str, option: &str, value: &Value) -> Self {
    Self(format!(
      "Invalid value {} for option \"{option}\" of rule \"{rule}\"",
      stringify(value)
    ))
  }

  /// Stylelint's answer to `false` where an option belongs.
  pub fn disabled_with_false(rule: &str) -> Self {
    Self(format!(
      "Invalid option value \"false\" for rule \"{rule}\". Are you trying to disable this rule? If so use \"null\" instead"
    ))
  }

  /// A regular expression that does not compile, with the reason (see
  /// [`pattern::invalid_option`]).
  pub fn pattern(rule: &str, pattern: &str, reason: &str) -> Self {
    Self(pattern::invalid_option_text(rule, pattern, reason))
  }

  /// The problem's text.
  pub fn text(&self) -> &str {
    &self.0
  }

  /// The problem's text, owned.
  pub fn into_text(self) -> String {
    self.0
  }
}

impl std::fmt::Display for InvalidOption {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.0)
  }
}

/// `value` quoted as Stylelint's `validateOptions` quotes it: a string as
/// it is, anything else as `JSON.stringify` writes it.
pub fn stringify(value: &Value) -> String {
  match value {
    Value::String(text) => format!("\"{text}\""),
    other => format!("\"{}\"", json_stringify(other)),
  }
}

/// `JSON.stringify(value)`: numbers as JavaScript prints them (`1`, not
/// `1.0`), and objects with their keys in the order they were written in.
pub fn json_stringify(value: &Value) -> String {
  match value {
    Value::Null => "null".to_string(),
    Value::Bool(flag) => flag.to_string(),
    Value::Number(number) => match (number.as_i64(), number.as_u64(), number.as_f64()) {
      (Some(int), _, _) => int.to_string(),
      (_, Some(int), _) => int.to_string(),
      (_, _, Some(float)) => js_number::to_js_string(float),
      _ => number.to_string(),
    },
    Value::String(text) => Value::String(text.clone()).to_string(),
    Value::Array(items) => {
      let items: Vec<String> = items.iter().map(json_stringify).collect();
      format!("[{}]", items.join(","))
    }
    Value::Object(map) => {
      let entries: Vec<String> = map
        .iter()
        .map(|(key, item)| format!("{}:{}", Value::String(key.clone()), json_stringify(item)))
        .collect();
      format!("{{{}}}", entries.join(","))
    }
  }
}

/// One kind of value an option accepts: an entry of the `possible` lists in
/// Stylelint's `validateOptions` calls.
#[derive(Clone, Copy, Debug)]
pub enum Accept {
  /// Exactly this string (`'always'`).
  Exactly(&'static str),
  /// `true`.
  True,
  /// Any string: Stylelint's `isString`.  It also stands for `isRegExp`,
  /// since a regular expression reaches gale as a `/.../` string.
  String,
  /// `true` or `false`: `isBoolean`.
  Boolean,
  /// Any number: `isNumber`.
  Number,
  /// A whole number of zero or more: `isNonNegativeInteger`.
  NonNegativeInteger,
  /// Whatever the function accepts, for a validator of the rule's own.
  Test(fn(&Value) -> bool),
}

impl Accept {
  /// Whether `value` is of this kind.
  pub fn accepts(&self, value: &Value) -> bool {
    match self {
      Accept::Exactly(expected) => value.as_str() == Some(*expected),
      Accept::True => value == &Value::Bool(true),
      Accept::String => value.is_string(),
      Accept::Boolean => value.is_boolean(),
      Accept::Number => value.is_number(),
      Accept::NonNegativeInteger => value.as_f64().is_some_and(|n| n >= 0.0 && n.fract() == 0.0),
      Accept::Test(test) => test(value),
    }
  }
}

/// Stylelint's `[isString, isRegExp]`: a name or a `/regex/`, or a list of
/// them.
pub const STRING_OR_REGEX: &[Accept] = &[Accept::String];

/// Whether any of `possible` accepts `value`.
fn accepted(possible: &[Accept], value: &Value) -> bool {
  possible.iter().any(|accept| accept.accepts(value))
}

/// `[value].flat()`: the entries of an array, or the value itself.
fn flat(value: &Value) -> impl Iterator<Item = &Value> {
  match value {
    Value::Array(items) => items.iter().collect::<Vec<_>>().into_iter(),
    other => vec![other].into_iter(),
  }
}

/// JavaScript's `Boolean(value)`, for a rule whose `possible` is `Boolean`.
pub fn is_truthy(value: &Value) -> bool {
  match value {
    Value::Null => false,
    Value::Bool(flag) => *flag,
    Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
    Value::String(text) => !text.is_empty(),
    Value::Array(_) | Value::Object(_) => true,
  }
}

/// Stylelint's `validateObjectWithArrayProps(isString, isRegExp)`: an
/// object whose every value is a string or a list of strings.
pub fn is_object_of_string_lists(value: &Value) -> bool {
  value.as_object().is_some_and(is_map_of_string_lists)
}

/// [`is_object_of_string_lists`] for an object already in hand.
pub fn is_map_of_string_lists(map: &Map<String, Value>) -> bool {
  map.values().all(|item| match item {
    Value::Array(items) => items.iter().all(Value::is_string),
    other => other.is_string(),
  })
}

/// Stylelint's `validateObjectWithProps(isString)`: an object whose every
/// value is a string.
pub fn is_object_of_strings(value: &Value) -> bool {
  value
    .as_object()
    .is_some_and(|map| map.values().all(Value::is_string))
}

/// Stylelint's `validateObjectWithProps(isNonNegativeInteger)`: an object
/// whose every value is a whole number of zero or more.
pub fn is_object_of_non_negative_integers(value: &Value) -> bool {
  value.as_object().is_some_and(|map| {
    map
      .values()
      .all(|item| Accept::NonNegativeInteger.accepts(item))
  })
}

/// A rule's setting from the config, normalised the way Stylelint's
/// `normalizeRuleSettings` does it: a primary option and, perhaps, a
/// secondary options object.
///
/// Gale reads two shapes Stylelint does not, which its presets and
/// existing configs use:
///
/// - `[true, setting]` is `setting`, as `["error", setting]` is (the config
///   takes the severity off that one), and `[true]` is `true`: the rule
///   with its default options.  Stylelint would read `true` as the primary
///   option.
/// - A bare `{...}` is the secondary options of a rule whose primary option
///   is not an object, with its default primary option.  A rule whose
///   primary option is an object (its meta's `primary_option_object`) reads
///   it as Stylelint does, as the primary option.
#[derive(Debug, Clone, PartialEq)]
pub struct Setting {
  rule: &'static str,
  /// The primary option, or `None` when the config gives none: a rule set
  /// to `true` or to a severity, or enabled by a preset with its defaults.
  /// Gale lets such a rule run with its default primary option.
  primary: Option<Value>,
  /// The secondary options as written, which need not be an object.
  secondary: Option<Value>,
  /// Whether the rule's primary option is an object, which only
  /// [`Options::primary_object`] may read.
  primary_option_object: bool,
}

impl Setting {
  /// The setting for the rule `meta` describes from `options`, the setting
  /// as the config wrote it (without gale's `["error", ...]` severity).
  /// The meta's `primary_option_array` is Stylelint's `primaryOptionArray`:
  /// whether a primary option can itself be an array, which decides
  /// whether `["a", "b"]` is a list or a primary option followed by a
  /// secondary one.
  pub fn new(meta: &RuleMeta, options: Option<&Value>) -> Self {
    let mut setting = Self {
      rule: meta.name,
      primary: None,
      secondary: None,
      primary_option_object: meta.primary_option_object,
    };
    match options {
      None => {}
      // Stylelint turns a rule set to `[null, ...]` off; gale's config
      // model has already decided whether it runs, so it has no options.
      Some(Value::Array(items)) if items.first().is_some_and(Value::is_null) => {}
      Some(Value::Array(items)) if items.first() == Some(&Value::Bool(true)) => {
        return Self::new(meta, items.get(1));
      }
      Some(Value::Object(_)) if !meta.primary_option_object => {
        setting.secondary = options.cloned();
      }
      Some(Value::Array(items)) if !meta.primary_option_array => {
        setting.primary = items.first().cloned();
        setting.secondary = items.get(1).cloned();
      }
      Some(Value::Array(items)) => match items.as_slice() {
        [list @ Value::Array(_)] => setting.primary = Some(list.clone()),
        [primary, secondary @ Value::Object(_)] if !primary.is_object() => {
          setting.primary = Some(primary.clone());
          setting.secondary = Some(secondary.clone());
        }
        _ => setting.primary = options.cloned(),
      },
      Some(primary) => setting.primary = Some(primary.clone()),
    }
    setting
  }

  /// The name of the rule the setting is for, under which its problems are
  /// worded.
  pub fn rule(&self) -> &'static str {
    self.rule
  }

  /// The primary option, if the config gives one.
  pub fn primary(&self) -> Option<&Value> {
    self.primary.as_ref()
  }

  /// The secondary options as written, if any.
  pub fn secondary(&self) -> Option<&Value> {
    self.secondary.as_ref()
  }

  /// A reader for the options, which validates what the rule reads.
  pub fn options(&self) -> Options<'_> {
    Options {
      setting: self,
      primary: self.primary.as_ref(),
      secondary: self.secondary.as_ref(),
      takes_secondary: false,
      problems: Vec::new(),
      key_problems: Vec::new(),
      read_keys: Vec::new(),
    }
  }
}

/// Reads a [`Setting`] for a rule, validating every value it hands out the
/// way Stylelint's `validateOptions` does, and collecting the problems.
///
/// The primary option is read with one of the `primary_*` methods.  Each
/// secondary option is read by its key; reading any of them means the rule
/// takes secondary options, and [`Options::finish`] then reports every key
/// the rule did not read as unknown (except [`FRAMEWORK_KEYS`]).  A rule
/// that reads no secondary option ignores whatever secondary options it is
/// given, as Stylelint does for a rule that validates none.
///
/// A value that is invalid is reported and read as missing, so the rule
/// falls back on its default; the rule does not run anyway once
/// [`Options::finish`] reports a problem.
pub struct Options<'s> {
  setting: &'s Setting,
  primary: Option<&'s Value>,
  secondary: Option<&'s Value>,
  /// Whether the rule has read a secondary option.
  takes_secondary: bool,
  /// Problems with the primary option, in the order found.
  problems: Vec<InvalidOption>,
  /// Problems with secondary options, with the key each is about.
  key_problems: Vec<(&'static str, InvalidOption)>,
  /// The secondary options the rule has read.
  read_keys: Vec<&'static str>,
}

impl<'s> Options<'s> {
  /// The rule's name.
  fn rule(&self) -> &'static str {
    self.setting.rule
  }

  /// The primary option, after the check every value gets first: `false`
  /// is Stylelint's way of asking whether the rule was meant to be off.
  fn primary_checked(&mut self) -> Option<&'s Value> {
    let primary = self.primary?;
    if primary == &Value::Bool(false) {
      self
        .problems
        .push(InvalidOption::disabled_with_false(self.rule()));
      return None;
    }
    Some(primary)
  }

  /// Validate the primary option against `possible`, as Stylelint does for
  /// `{ actual: primary, possible: [...] }`: every entry of an array must
  /// be accepted.  Returns the option when it is.
  pub fn primary_value(&mut self, possible: &[Accept]) -> Option<&'s Value> {
    let primary = self.primary_checked()?;
    let mut valid = true;
    for entry in flat(primary) {
      if !accepted(possible, entry) {
        self.problems.push(InvalidOption::value(self.rule(), entry));
        valid = false;
      }
    }
    valid.then_some(primary)
  }

  /// A primary option that must be `true` (Stylelint's `possible: [true]`),
  /// for a rule that has no other.
  pub fn primary_true(&mut self) {
    self.primary_value(&[Accept::True]);
  }

  /// A rule that takes no primary option but `true`: Stylelint validates
  /// `{ actual: primary }` with nothing possible, and words a problem as
  /// `Unexpected option value`.
  pub fn no_primary(&mut self) {
    if let Some(primary) = self.primary_checked()
      && primary != &Value::Bool(true)
    {
      self
        .problems
        .push(InvalidOption::unexpected(self.rule(), primary));
    }
  }

  /// A primary option that is one of `allowed`.  `None` when the config
  /// gives none, or one that is not allowed.
  pub fn primary_one_of(&mut self, allowed: &[&'static str]) -> Option<&'static str> {
    let possible: Vec<Accept> = allowed.iter().map(|&word| Accept::Exactly(word)).collect();
    let primary = self.primary_value(&possible)?;
    allowed
      .iter()
      .copied()
      .find(|&word| primary.as_str() == Some(word))
  }

  /// A number for the primary option, accepted by `possible` (usually
  /// [`Accept::Number`] or [`Accept::NonNegativeInteger`]).
  pub fn primary_number(&mut self, possible: &[Accept]) -> Option<f64> {
    self.primary_value(possible)?.as_f64()
  }

  /// A primary option that `test` accepts, when Stylelint's `possible` is a
  /// function rather than a list: the option is tested whole, and a problem
  /// is worded `Invalid option "<value>"`.
  pub fn primary_test(&mut self, test: fn(&Value) -> bool) -> Option<&'s Value> {
    let primary = self.primary_checked()?;
    if test(primary) {
      Some(primary)
    } else {
      self
        .problems
        .push(InvalidOption::rejected(self.rule(), primary));
      None
    }
  }

  /// A primary list of names and `/regex/` literals (Stylelint's
  /// `[isString, isRegExp]`), as written.  A single string is a list of
  /// one.
  pub fn primary_strings(&mut self) -> Option<Vec<String>> {
    let primary = self.primary_value(STRING_OR_REGEX)?;
    Some(strings_of(primary))
  }

  /// A primary option that is a regular expression (Stylelint's
  /// `[isRegExp, isString]`), compiled.  One that does not compile is
  /// reported with the reason, as [`pattern::invalid_option_text`] words it.
  pub fn primary_pattern(&mut self) -> Option<Pattern> {
    let source = self.primary_value(STRING_OR_REGEX)?.as_str()?;
    match pattern::compile(source) {
      Ok(regex) => Some(Pattern {
        source: source.to_string(),
        regex,
      }),
      Err(reason) => {
        self
          .problems
          .push(InvalidOption::pattern(self.rule(), source, &reason));
        None
      }
    }
  }

  /// A primary option that is an object, such as a map from property names
  /// to lists, for a rule whose meta says `primary_option_object`.
  pub fn primary_object(&mut self) -> Option<&'s Map<String, Value>> {
    debug_assert!(
      self.setting.primary_option_object,
      "{} reads an object primary option, so its meta must say `primary_option_object`",
      self.rule()
    );
    let primary = self.primary_checked()?;
    match primary {
      Value::Object(map) => Some(map),
      other => {
        self.problems.push(InvalidOption::value(self.rule(), other));
        None
      }
    }
  }

  /// The secondary option `key`, validated against `possible` (every entry
  /// of an array must be accepted).  `None` when it is missing or invalid.
  pub fn value(&mut self, key: &'static str, possible: &[Accept]) -> Option<&'s Value> {
    self.takes_secondary = true;
    if !self.read_keys.contains(&key) {
      self.read_keys.push(key);
    }
    let value = self.secondary?.as_object()?.get(key)?;
    let mut valid = true;
    for entry in flat(value) {
      if !accepted(possible, entry) {
        let problem = InvalidOption::option_value(self.rule(), key, entry);
        self.key_problems.push((key, problem));
        valid = false;
      }
    }
    valid.then_some(value)
  }

  /// A secondary option that is `true` or `false` (Stylelint's
  /// `[isBoolean]`); `false` when missing.
  pub fn flag(&mut self, key: &'static str) -> bool {
    self.value(key, &[Accept::Boolean]) == Some(&Value::Bool(true))
  }

  /// A secondary option listing keywords from `allowed`, such as
  /// `ignore: ["comments"]`: the ones it lists.
  pub fn keywords(&mut self, key: &'static str, allowed: &[&'static str]) -> Vec<&'static str> {
    let possible: Vec<Accept> = allowed.iter().map(|&word| Accept::Exactly(word)).collect();
    let Some(value) = self.value(key, &possible) else {
      return Vec::new();
    };
    allowed
      .iter()
      .copied()
      .filter(|&word| flat(value).any(|entry| entry.as_str() == Some(word)))
      .collect()
  }

  /// A secondary option listing names and `/regex/` literals (Stylelint's
  /// `[isString, isRegExp]`), as written.
  pub fn strings(&mut self, key: &'static str) -> Vec<String> {
    self
      .value(key, STRING_OR_REGEX)
      .map(strings_of)
      .unwrap_or_default()
  }

  /// A secondary option listing names and `/regex/` literals, compiled to
  /// match with (see [`Matchers`]).
  pub fn matchers(&mut self, key: &'static str) -> Matchers {
    Matchers::new(&self.strings(key))
  }

  /// A number for the secondary option `key`, accepted by `possible`.
  pub fn number(&mut self, key: &'static str, possible: &[Accept]) -> Option<f64> {
    self.value(key, possible)?.as_f64()
  }

  /// `disableFix` (`[isBoolean]`), for a rule that validates it as its own
  /// secondary option, so that every other key is unknown.  The runner
  /// applies it, as it does for any rule.
  pub fn disable_fix(&mut self) {
    self.flag("disableFix");
  }

  /// The secondary option `key`, a name or `/regex/` entry or a list of
  /// them, read without taking secondary options: any other key stays
  /// ignored, as it does for one of gale's own rules that is lenient.  An
  /// entry that is not a string is reported as Stylelint words it.
  pub fn lenient_strings(&mut self, key: &str) -> Vec<String> {
    let Some(value) = self.secondary.and_then(|secondary| secondary.get(key)) else {
      return Vec::new();
    };
    let mut strings = Vec::new();
    for entry in flat(value) {
      match entry.as_str() {
        Some(text) => strings.push(text.to_string()),
        None => {
          let problem = InvalidOption::option_value(self.rule(), key, entry);
          self.problems.push(problem);
        }
      }
    }
    strings
  }

  /// Report a problem the rule found itself, such as a value that passes
  /// the type check but means nothing to the rule.
  pub fn report(&mut self, problem: InvalidOption) {
    self.problems.push(problem);
  }

  /// The problems found, if any: those with the primary option first, then
  /// those with each secondary option in the order the options are held,
  /// with every key the rule did not read reported as unknown.
  pub fn finish(self) -> Result<(), Vec<InvalidOption>> {
    let rule = self.rule();
    let Options {
      secondary,
      takes_secondary,
      mut problems,
      mut key_problems,
      read_keys,
      ..
    } = self;
    if takes_secondary && let Some(secondary) = secondary {
      match secondary {
        Value::Null => {}
        Value::Bool(false) => problems.push(InvalidOption::disabled_with_false(rule)),
        Value::Object(map) => {
          for key in map.keys() {
            if read_keys.contains(&key.as_str()) {
              let mut index = 0;
              while index < key_problems.len() {
                if key_problems[index].0 == key.as_str() {
                  problems.push(key_problems.remove(index).1);
                } else {
                  index += 1;
                }
              }
            } else if !FRAMEWORK_KEYS.contains(&key.as_str()) {
              problems.push(InvalidOption::name(rule, key));
            }
          }
        }
        other => problems.push(InvalidOption::not_an_object(rule, other)),
      }
    }
    if problems.is_empty() {
      Ok(())
    } else {
      Err(problems)
    }
  }
}

/// The strings of a value that is a string or a list of them; other
/// entries, which validation has already reported, are left out.
fn strings_of(value: &Value) -> Vec<String> {
  flat(value)
    .filter_map(|entry| entry.as_str().map(str::to_string))
    .collect()
}

/// A regular expression from a `*-pattern` rule's primary option, with the
/// text it was compiled from (which messages quote).
#[derive(Debug, Clone)]
pub struct Pattern {
  /// The option as written.
  pub source: String,
  /// The compiled expression.
  pub regex: Arc<Regex>,
}

impl Pattern {
  /// `source` compiled (see [`pattern::compile`]), or why it does not
  /// compile.  For a rule's default pattern; one from the config is read
  /// with [`Options::primary_pattern`].
  pub fn compile(source: &str) -> Result<Self, String> {
    Ok(Self {
      source: source.to_string(),
      regex: pattern::compile(source)?,
    })
  }

  /// Whether the pattern matches `text` (see [`pattern::is_match`]).
  pub fn is_match(&self, text: &str) -> bool {
    pattern::is_match(&self.regex, text)
  }
}

/// One entry of a list option: a name, or a `/regex/` literal.
#[derive(Debug, Clone)]
enum Matcher {
  /// A name, which must equal the text exactly.
  Exact(String),
  /// A regex literal, compiled.
  Regex(Arc<Regex>),
  /// A regex literal that does not compile, which matches nothing.  The
  /// rule set reports it as an invalid option without stopping the rule.
  Broken,
}

/// A list of names and `/regex/` literals, compiled once: Stylelint's
/// `matchesStringOrRegExp` and `optionsMatches`.
#[derive(Debug, Clone, Default)]
pub struct Matchers {
  entries: Vec<Matcher>,
  /// Whether names match whatever their ASCII case.
  ignore_case: bool,
}

impl Matchers {
  /// The matchers for `entries`, each a name or a `/regex/` literal.
  pub fn new<S: AsRef<str>>(entries: &[S]) -> Self {
    Self {
      entries: Self::compile(entries),
      ignore_case: false,
    }
  }

  /// The matchers for `entries`, whose names match whatever their ASCII
  /// case, for a rule that compares names case-insensitively.  A regex
  /// matches as written, flags and all.
  pub fn ignoring_case<S: AsRef<str>>(entries: &[S]) -> Self {
    Self {
      entries: Self::compile(entries),
      ignore_case: true,
    }
  }

  /// Each of `entries` compiled.
  fn compile<S: AsRef<str>>(entries: &[S]) -> Vec<Matcher> {
    entries
      .iter()
      .map(|entry| {
        let entry = entry.as_ref();
        if pattern::split_literal(entry).is_none() {
          Matcher::Exact(entry.to_string())
        } else {
          pattern::compile(entry).map_or(Matcher::Broken, Matcher::Regex)
        }
      })
      .collect()
  }

  /// Whether any entry matches `text`: a name by equality (in any case for
  /// [`Matchers::ignoring_case`]), a regex by search.
  pub fn matches(&self, text: &str) -> bool {
    self.entries.iter().any(|matcher| match matcher {
      Matcher::Exact(name) if self.ignore_case => name.eq_ignore_ascii_case(text),
      Matcher::Exact(name) => name == text,
      Matcher::Regex(regex) => pattern::is_match(regex, text),
      Matcher::Broken => false,
    })
  }

  /// Whether the list has no entries.
  pub fn is_empty(&self) -> bool {
    self.entries.is_empty()
  }
}

/// The letter case a `*-case` rule expects: its primary option, `"lower"`
/// or `"upper"`, read with [`Options::primary_case`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LetterCase {
  /// `"lower"`, the default.
  Lower,
  /// `"upper"`.
  Upper,
}

impl LetterCase {
  /// Whether `text` holds an ASCII letter in the other case.
  pub fn is_wrong(self, text: &str) -> bool {
    match self {
      LetterCase::Lower => text.bytes().any(|b| b.is_ascii_uppercase()),
      LetterCase::Upper => text.bytes().any(|b| b.is_ascii_lowercase()),
    }
  }

  /// `text` with its ASCII letters in this case.
  pub fn apply(self, text: &str) -> String {
    match self {
      LetterCase::Lower => text.to_ascii_lowercase(),
      LetterCase::Upper => text.to_ascii_uppercase(),
    }
  }

  /// The case as messages name it: `lowercase` or `uppercase`.
  pub fn word(self) -> &'static str {
    match self {
      LetterCase::Lower => "lowercase",
      LetterCase::Upper => "uppercase",
    }
  }
}

/// Readers shared by families of rules: the `*-case`, `*-pattern` and
/// `selector-max-*` rules.
impl Options<'_> {
  /// A `*-case` rule's primary option, `"lower"` or `"upper"`.  Gale reads
  /// no option (or one that is not allowed, which is reported) as
  /// `"lower"`.
  pub fn primary_case(&mut self) -> LetterCase {
    match self.primary_one_of(&["lower", "upper"]) {
      Some("upper") => LetterCase::Upper,
      _ => LetterCase::Lower,
    }
  }

  /// A `*-pattern` rule's primary option, compiled (see
  /// [`Options::primary_pattern`]), or `default` when the config gives
  /// none.  A pattern that does not compile is reported, so the rule does
  /// not run and the default only stands in until [`Options::finish`].
  pub fn primary_pattern_or(&mut self, default: &str) -> Pattern {
    self
      .primary_pattern()
      .unwrap_or_else(|| Pattern::compile(default).expect("the default pattern compiles"))
  }

  /// A `selector-max-*` rule's primary option: a whole number of zero or
  /// more, tested by a function as Stylelint tests it, so a problem reads
  /// `Invalid option`.  It is read as a float, so that `5.0`, which a
  /// config may write for 5, counts as 5 rather than falling back to
  /// `default`, which stands for no option.
  pub fn primary_max(&mut self, default: usize) -> usize {
    self
      .primary_test(|value| Accept::NonNegativeInteger.accepts(value))
      .and_then(Value::as_f64)
      .map_or(default, |max| max as usize)
  }
}

#[cfg(test)]
mod tests;
