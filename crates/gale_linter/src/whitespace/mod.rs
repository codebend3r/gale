//! The whitespace checker the `*-space-before/after`, `*-newline-before/after`
//! rules share.
//!
//! The @stylistic plugin builds about forty of its rules on one utility,
//! `whitespaceChecker(targetWhitespace, expectation, messages)`: a rule
//! finds the characters it is about (the commas of a selector list, the
//! colon of a declaration, the `{` of a block) and asks the checker whether
//! the whitespace before or after each one meets the expectation.  This is
//! that utility, ported line for line: [`Checker`] with its `before`,
//! `before_allowing_indentation`, `after` and `after_one_only` checks,
//! [`Expectation`], the single-line test, and the [`Messages`] each rule
//! words its problems with.  stylelint-scss carries an older copy with a few
//! differences (a newline before the index counts, the end of the text does
//! not end the check, `at-least-one-space`); [`Checker::scss`] follows it.
//!
//! [`style_search`] ports `style-search`, the scanner most of the rules find
//! their characters with, and [`SpacingRule`] turns a rule into a short
//! declaration: its options, its messages and the check that runs the
//! checker over the places its family finds.
//!
//! Indexes are byte offsets.  The plugin counts UTF-16 units, but every
//! character the checker looks at is ASCII, so the two agree on what sits
//! next to the index, and a byte offset added to a node's start lands where
//! PostCSS's `positionInside` does.

pub mod style_search;

use std::marker::PhantomData;
use std::ops::Range;

use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::chars::is_css_whitespace_byte as is_whitespace;
use crate::empty_lines::newline_of;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Options, Setting};
use crate::rule::{Check, RuleContext};

pub use style_search::{Match, Mode, Needle, SearchOptions, style_search};

/// The whitespace a rule is about (`targetWhitespace`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
  /// One space.
  Space,
  /// A line break, `\n` or `\r\n`.
  Newline,
}

/// What the rule expects next to the characters it checks: the rule's
/// primary option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expectation {
  /// `"always"`.
  Always,
  /// `"never"`.
  Never,
  /// `"always-single-line"`: always, where the text is on one line.
  AlwaysSingleLine,
  /// `"never-single-line"`.
  NeverSingleLine,
  /// `"always-multi-line"`: always, where the text spans lines.
  AlwaysMultiLine,
  /// `"never-multi-line"`.
  NeverMultiLine,
  /// stylelint-scss's `"at-least-one-space"`: a space after, however many.
  AtLeastOneSpace,
}

impl Expectation {
  /// The expectation an option names.  Options are validated before a rule
  /// is configured, so anything else is a bug.
  pub fn parse(option: &str) -> Self {
    match option {
      "always" => Self::Always,
      "never" => Self::Never,
      "always-single-line" => Self::AlwaysSingleLine,
      "never-single-line" => Self::NeverSingleLine,
      "always-multi-line" => Self::AlwaysMultiLine,
      "never-multi-line" => Self::NeverMultiLine,
      "at-least-one-space" => Self::AtLeastOneSpace,
      _ => unreachable!("unknown whitespace expectation {option:?}"),
    }
  }

  /// Whether the option starts with `always` (`primary.startsWith('always')`
  /// in the rules' fixes).
  pub fn is_always(self) -> bool {
    matches!(
      self,
      Self::Always | Self::AlwaysSingleLine | Self::AlwaysMultiLine
    )
  }

  /// Whether the option starts with `never`.
  pub fn is_never(self) -> bool {
    matches!(
      self,
      Self::Never | Self::NeverSingleLine | Self::NeverMultiLine
    )
  }
}

/// The primary options of a space rule that checks single-line lists.
pub const SPACE_SINGLE_LINE: &[&str] =
  &["always", "never", "always-single-line", "never-single-line"];

/// The primary options of a newline rule that checks multi-line lists.
pub const NEWLINE_MULTI_LINE: &[&str] = &["always", "always-multi-line", "never-multi-line"];

/// The primary options of a rule that is either on or off.
pub const ALWAYS_NEVER: &[&str] = &["always", "never"];

/// The primary options of a rule that checks blocks of either kind.
pub const SPACE_ANY_LINE: &[&str] = &[
  "always",
  "never",
  "always-single-line",
  "never-single-line",
  "always-multi-line",
  "never-multi-line",
];

/// What a rule says about each kind of problem, as the plugin's `messages`
/// object holds them for `whitespaceChecker`.  A `%s` in a message is
/// replaced by what the problem is about: the checked character, or the
/// `errTarget` the rule passes (an at-rule name, a combinator).
///
/// A message the rule's options never reach is left out, as the plugin
/// leaves it out; a single- or multi-line expectation without its own
/// message uses the plain one.
#[derive(Debug, Clone, Copy, Default)]
pub struct Messages {
  pub expected_before: Option<&'static str>,
  pub rejected_before: Option<&'static str>,
  pub expected_after: Option<&'static str>,
  pub rejected_after: Option<&'static str>,
  pub expected_before_single_line: Option<&'static str>,
  pub rejected_before_single_line: Option<&'static str>,
  pub expected_before_multi_line: Option<&'static str>,
  pub rejected_before_multi_line: Option<&'static str>,
  pub expected_after_single_line: Option<&'static str>,
  pub rejected_after_single_line: Option<&'static str>,
  pub expected_after_multi_line: Option<&'static str>,
  pub rejected_after_multi_line: Option<&'static str>,
  /// stylelint-scss's `expectedAfterAtLeast`, for `at-least-one-space`.
  pub expected_after_at_least: Option<&'static str>,
}

impl Messages {
  /// No messages, to fill in the rest of a `Messages { .. }` literal.
  pub const NONE: Self = Self {
    expected_before: None,
    rejected_before: None,
    expected_after: None,
    rejected_after: None,
    expected_before_single_line: None,
    rejected_before_single_line: None,
    expected_before_multi_line: None,
    rejected_before_multi_line: None,
    expected_after_single_line: None,
    rejected_after_single_line: None,
    expected_after_multi_line: None,
    rejected_after_multi_line: None,
    expected_after_at_least: None,
  };
}

/// Fill in a message's `%s`.
fn render(message: Option<&'static str>, target: &str) -> Option<String> {
  message.map(|message| message.replacen("%s", target, 1))
}

/// The plugin's `isSingleLineString`: no `\n` or `\r` in `text`.
pub fn is_single_line(text: &str) -> bool {
  !text.contains(['\n', '\r'])
}

/// Whether `c` matches JavaScript's `\s`, which the rules' fixes trim with.
pub fn is_js_space(c: char) -> bool {
  matches!(
    c,
    '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
      ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
  )
}

/// Whether `c` is whitespace other than a line break (`[^\S\r\n]`).
pub fn is_horizontal_space(c: char) -> bool {
  is_js_space(c) && c != '\r' && c != '\n'
}

/// The range of the `\s*` run that starts at byte `from` of `text`.
pub fn space_run_after(text: &str, from: usize) -> Range<usize> {
  let rest = text.get(from..).unwrap_or("");
  let len = rest.len() - rest.trim_start_matches(is_js_space).len();
  from..from + len
}

/// The range of the `\s*` run that ends at byte `to` of `text`.
pub fn space_run_before(text: &str, to: usize) -> Range<usize> {
  let head = text.get(..to).unwrap_or("");
  head.trim_end_matches(is_js_space).len()..to
}

/// One place to check: the text the rule hands the checker (`source`), the
/// index in it, and the optional `errTarget` and `lineCheckStr`.
///
/// The index is signed because rules check what starts a node by asking
/// about the character after index -1.
#[derive(Debug, Clone, Copy)]
pub struct At<'s> {
  pub source: &'s str,
  pub index: isize,
  pub err_target: Option<&'s str>,
  pub line_check: Option<&'s str>,
}

impl<'s> At<'s> {
  /// Index `index` of `source`.
  pub fn new(source: &'s str, index: usize) -> Self {
    Self {
      source,
      index: index as isize,
      err_target: None,
      line_check: None,
    }
  }

  /// What starts `source`: the character after index -1.
  pub fn start_of(source: &'s str) -> Self {
    Self {
      index: -1,
      ..Self::new(source, 0)
    }
  }

  /// With `errTarget` set.
  pub fn err_target(self, target: &'s str) -> Self {
    Self {
      err_target: Some(target),
      ..self
    }
  }

  /// With `lineCheckStr` set.
  pub fn line_check(self, text: &'s str) -> Self {
    Self {
      line_check: Some(text),
      ..self
    }
  }

  /// The byte `offset` places from the index, if it is inside the text.
  fn byte(&self, offset: isize) -> Option<u8> {
    let at = usize::try_from(self.index + offset).ok()?;
    self.source.as_bytes().get(at).copied()
  }

  /// The text the single- and multi-line expectations look at:
  /// `lineCheckStr || source`.
  fn line_text(&self) -> &'s str {
    self
      .line_check
      .filter(|text| !text.is_empty())
      .unwrap_or(self.source)
  }
}

/// How a check treats the whitespace it looks at.
#[derive(Debug, Clone, Copy, Default)]
struct Look {
  /// `onlyOneChar`: look at one character, not at the one after it.
  only_one_char: bool,
  /// `allowIndentation`: spaces and tabs may sit between the expected
  /// newline and the index.
  allow_indentation: bool,
}

/// The plugin's `whitespaceChecker`, configured for one rule.
#[derive(Debug, Clone, Copy)]
pub struct Checker {
  target: Target,
  expectation: Expectation,
  messages: &'static Messages,
  /// Whether this is stylelint-scss's copy of the checker.
  scss: bool,
}

impl Checker {
  /// `whitespaceChecker(target, expectation, messages)`.
  pub fn new(target: Target, expectation: Expectation, messages: &'static Messages) -> Self {
    Self {
      target,
      expectation,
      messages,
      scss: false,
    }
  }

  /// The same checker as stylelint-scss's copy of the utility runs it.
  pub fn scss(self) -> Self {
    Self { scss: true, ..self }
  }

  /// The expectation the checker was built with.
  pub fn expectation(&self) -> Expectation {
    self.expectation
  }

  /// `before()`: the message for a problem with the whitespace before the
  /// index, if there is one.
  pub fn before(&self, at: At) -> Option<String> {
    self.check_before(at, Look::default())
  }

  /// `beforeAllowingIndentation()`: [`Self::before`], with spaces and tabs
  /// allowed between the expected whitespace and the index.
  pub fn before_allowing_indentation(&self, at: At) -> Option<String> {
    self.check_before(
      at,
      Look {
        allow_indentation: true,
        ..Look::default()
      },
    )
  }

  /// `after()`: the message for a problem with the whitespace after the
  /// index, if there is one.
  pub fn after(&self, at: At) -> Option<String> {
    self.check_after(at, Look::default())
  }

  /// `afterOneOnly()`: [`Self::after`], looking at one character only.
  pub fn after_one_only(&self, at: At) -> Option<String> {
    self.check_after(
      at,
      Look {
        only_one_char: true,
        ..Look::default()
      },
    )
  }

  /// Whether the single- or multi-line part of the expectation applies.
  fn applies(&self, at: &At) -> bool {
    let single = is_single_line(at.line_text());
    match self.expectation {
      Expectation::AlwaysSingleLine | Expectation::NeverSingleLine => single,
      Expectation::AlwaysMultiLine | Expectation::NeverMultiLine => !single,
      _ => true,
    }
  }

  fn check_before(&self, at: At, look: Look) -> Option<String> {
    if !self.applies(&at) {
      return None;
    }
    // A missing message falls back to the plain one, as the plugin's
    // `expectBefore(messageFunc = messages.expectedBefore)` does.
    let m = self.messages;
    let expected = |message: Option<&'static str>| message.or(m.expected_before);
    let rejected = |message: Option<&'static str>| message.or(m.rejected_before);
    match self.expectation {
      Expectation::Always => self.expect_before(&at, look, m.expected_before),
      Expectation::Never => self.reject_before(&at, m.rejected_before),
      Expectation::AlwaysSingleLine => {
        self.expect_before(&at, look, expected(m.expected_before_single_line))
      }
      Expectation::NeverSingleLine => {
        self.reject_before(&at, rejected(m.rejected_before_single_line))
      }
      Expectation::AlwaysMultiLine => {
        self.expect_before(&at, look, expected(m.expected_before_multi_line))
      }
      Expectation::NeverMultiLine => {
        self.reject_before(&at, rejected(m.rejected_before_multi_line))
      }
      // stylelint-scss's `before()` has no case for it and throws.
      Expectation::AtLeastOneSpace => None,
    }
  }

  fn check_after(&self, at: At, look: Look) -> Option<String> {
    if !self.applies(&at) {
      return None;
    }
    let m = self.messages;
    let expected = |message: Option<&'static str>| message.or(m.expected_after);
    let rejected = |message: Option<&'static str>| message.or(m.rejected_after);
    match self.expectation {
      Expectation::Always => self.expect_after(&at, look, m.expected_after),
      Expectation::Never => self.reject_after(&at, m.rejected_after),
      Expectation::AlwaysSingleLine => {
        self.expect_after(&at, look, expected(m.expected_after_single_line))
      }
      Expectation::NeverSingleLine => {
        self.reject_after(&at, rejected(m.rejected_after_single_line))
      }
      Expectation::AlwaysMultiLine => {
        self.expect_after(&at, look, expected(m.expected_after_multi_line))
      }
      Expectation::NeverMultiLine => self.reject_after(&at, rejected(m.rejected_after_multi_line)),
      Expectation::AtLeastOneSpace => {
        self.expect_after(&at, look, expected(m.expected_after_at_least))
      }
    }
  }

  /// The message, filled in with `errTarget` or the character at the index
  /// (`errTarget || source.charAt(index)`).
  fn problem(&self, at: &At, message: Option<&'static str>) -> Option<String> {
    let target = match at.err_target {
      Some(target) if !target.is_empty() => target,
      _ => usize::try_from(at.index)
        .ok()
        .and_then(|i| Some((i, at.source.get(i..)?.chars().next()?)))
        .map_or("", |(i, c)| &at.source[i..i + c.len_utf8()]),
    };
    render(message, target)
  }

  fn expect_before(&self, at: &At, look: Look, message: Option<&'static str>) -> Option<String> {
    if look.allow_indentation {
      return self.expect_before_allowing_indentation(at, message);
    }
    let one = at.byte(-1)?;
    let two = at.byte(-2);
    let ends = |byte: Option<u8>| look.only_one_char || !byte.is_some_and(is_whitespace);
    if self.scss && self.target == Target::Newline && one == b'\n' {
      // stylelint-scss accepts a line break, `\r\n` or `\n`, before the
      // index.
      let prev = if two == Some(b'\r') { at.byte(-3) } else { two };
      if ends(prev) {
        return None;
      }
    }
    if self.target == Target::Space && one == b' ' && ends(two) {
      return None;
    }
    self.problem(at, message)
  }

  fn expect_before_allowing_indentation(
    &self,
    at: &At,
    message: Option<&'static str>,
  ) -> Option<String> {
    // The plugin looks for `undefined` before a space target, which it finds
    // only before the start of the text; stylelint-scss looks for a space.
    let expected = match self.target {
      Target::Newline => Some(b'\n'),
      Target::Space if self.scss => Some(b' '),
      Target::Space => None,
    };
    let mut i = -1;
    loop {
      let byte = at.byte(i);
      if byte == expected {
        return None;
      }
      if matches!(byte, Some(b'\t' | b' ')) {
        i -= 1;
        continue;
      }
      return self.problem(at, message);
    }
  }

  /// Gale's own `"never"` for the newline rules, which upstream does not
  /// accept: no line break next to the character, spaces and tabs aside.
  /// The plugin's checker would read it as no whitespace at all, which is
  /// what the space rules are for.
  fn rejects_line_breaks_only(&self) -> bool {
    self.target == Target::Newline && self.expectation == Expectation::Never
  }

  fn reject_before(&self, at: &At, message: Option<&'static str>) -> Option<String> {
    if self.rejects_line_breaks_only() {
      let mut i = -1;
      while matches!(at.byte(i), Some(b' ' | b'\t')) {
        i -= 1;
      }
      return matches!(at.byte(i), Some(b'\n' | b'\r')).then(|| self.problem(at, message))?;
    }
    let one = at.byte(-1)?;
    is_whitespace(one).then(|| self.problem(at, message))?
  }

  fn expect_after(&self, at: &At, look: Look, message: Option<&'static str>) -> Option<String> {
    let one = at.byte(1);
    // The plugin stops at the end of the text; stylelint-scss reads past it
    // as an empty string and goes on to report.
    if one.is_none() && !self.scss {
      return None;
    }
    let two = at.byte(2);
    let ends = |byte: Option<u8>| look.only_one_char || !byte.is_some_and(is_whitespace);
    if self.target == Target::Newline {
      if one == Some(b'\r') && two == Some(b'\n') && ends(at.byte(3)) {
        return None;
      }
      if one == Some(b'\n') && ends(two) {
        return None;
      }
    }
    if self.target == Target::Space
      && one == Some(b' ')
      && (self.expectation == Expectation::AtLeastOneSpace || ends(two))
    {
      return None;
    }
    self.problem(at, message)
  }

  fn reject_after(&self, at: &At, message: Option<&'static str>) -> Option<String> {
    if self.rejects_line_breaks_only() {
      let mut i = 1;
      while matches!(at.byte(i), Some(b' ' | b'\t')) {
        i += 1;
      }
      return matches!(at.byte(i), Some(b'\n' | b'\r')).then(|| self.problem(at, message))?;
    }
    let one = at.byte(1)?;
    is_whitespace(one).then(|| self.problem(at, message))?
  }
}

/// A rule built on the [`Checker`]: what it accepts, what it says and the
/// check it runs.  [`Spacing`] configures it from its setting.
pub trait SpacingRule: Sized + Send + Sync + 'static {
  /// The primary options the rule accepts, gale's own included.
  const OPTIONS: &'static [&'static str];
  /// The option Gale applies when the config gives none, as `gale:all`
  /// does.
  const DEFAULT: &'static str;
  /// The whitespace the rule is about.
  const TARGET: Target;
  /// The rule's messages.
  const MESSAGES: &'static Messages;
  /// Whether the rule is stylelint-scss's, which runs its own copy of the
  /// checker.
  const SCSS: bool = false;

  /// The secondary options the rule reads.
  type Secondary: Send + Sync;

  /// Read the secondary options.  Rules without any read none, so that
  /// every key the rule does not know stays silent, as upstream.
  fn secondary(options: &mut Options) -> Self::Secondary;

  /// Check the file.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic>;
}

/// A [`SpacingRule`] configured with its options.
pub struct Spacing<R: SpacingRule> {
  /// The rule's name.
  pub name: &'static str,
  /// The checker, with the rule's expectation and messages.
  pub checker: Checker,
  /// The rule's secondary options.
  pub secondary: R::Secondary,
  rule: PhantomData<R>,
}

impl<R: SpacingRule> Spacing<R> {
  /// Read the rule's options: one of [`SpacingRule::OPTIONS`], then any
  /// secondary options.
  pub fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(R::OPTIONS).unwrap_or(R::DEFAULT);
    let secondary = R::secondary(&mut options);
    options.finish()?;
    let checker = Checker::new(R::TARGET, Expectation::parse(option), R::MESSAGES);
    Ok(Self {
      name: setting.rule(),
      checker: if R::SCSS { checker.scss() } else { checker },
      secondary,
      rule: PhantomData,
    })
  }

  /// The expectation the rule was configured with.
  pub fn expectation(&self) -> Expectation {
    self.checker.expectation()
  }

  /// A problem reported at `offset` of `source`, on the one character
  /// there, as Stylelint's `report({ index, endIndex: index })` puts it.
  pub fn problem(&self, source: &str, message: String, offset: usize) -> Diagnostic {
    Diagnostic::new(self.name, message).span(char_span(source, offset))
  }

  /// [`Self::problem`], fixed by `edits` when there are any.
  pub fn problem_fixed_by(
    &self,
    source: &str,
    message: String,
    offset: usize,
    edits: Vec<Edit>,
  ) -> Diagnostic {
    let problem = self.problem(source, message, offset);
    // An edit that inserts nothing changes nothing, but would keep an edit
    // at the same place from applying.
    let edits: Vec<Edit> = edits
      .into_iter()
      .filter(|edit| edit.span.length > 0 || !edit.new_text.is_empty())
      .collect();
    if edits.is_empty() {
      problem
    } else {
      problem.fix(Fix::new("Fix whitespace", edits))
    }
  }
}

impl<R: SpacingRule> Check for Spacing<R> {
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    R::check(self, tree, ctx)
  }
}

/// The span of the character at `offset` of `source`, where Stylelint
/// reports a problem at one index.  A line break gets an empty span, which
/// the formatter ends one column on, on the same line, as PostCSS does;
/// past the end of the text, where an index lands after postcss-scss
/// lengthened `//` comments, the span is one column wide, as PostCSS counts
/// on past the end.
pub fn char_span(source: &str, offset: usize) -> Span {
  let len = match source.get(offset..).and_then(|rest| rest.chars().next()) {
    Some('\n' | '\r') => 0,
    Some(c) => c.len_utf8(),
    None => 1,
  };
  Span::new(offset, len)
}

/// The line break the file's fixes insert (Stylelint's `context.newline`).
pub fn newline(tree: &PostcssTree) -> &'static str {
  newline_of(tree.source())
}

/// An edit replacing `range` of a text that starts at source offset `base`.
pub fn replace(base: usize, range: Range<usize>, text: impl Into<String>) -> Edit {
  Edit::new(Span::from_range(base + range.start, base + range.end), text)
}

/// The fix of the "space after" rules: the whitespace after the character
/// at `index` of `text` (at source offset `base`) becomes one space for an
/// `always` expectation and nothing for a `never` one
/// (`after.replace(/^\s*/, ' ')`).
pub fn fix_space_after(
  expectation: Expectation,
  base: usize,
  text: &str,
  index: usize,
) -> Vec<Edit> {
  let run = space_run_after(text, index + 1);
  let with = if expectation.is_always() {
    " "
  } else if expectation.is_never() {
    ""
  } else {
    return vec![];
  };
  vec![replace(base, run, with)]
}

/// The fix of the "space before" rules: the whitespace before the character
/// at `index` of `text` becomes one space or nothing
/// (`before.replace(/\s*$/, ' ')`).
pub fn fix_space_before(
  expectation: Expectation,
  base: usize,
  text: &str,
  index: usize,
) -> Vec<Edit> {
  let run = space_run_before(text, index);
  let with = if expectation.is_always() {
    " "
  } else if expectation.is_never() {
    ""
  } else {
    return vec![];
  };
  vec![replace(base, run, with)]
}

#[cfg(test)]
mod tests {
  use super::*;

  const MESSAGES: &Messages = &Messages {
    expected_before: Some("expected before %s"),
    rejected_before: Some("rejected before %s"),
    expected_after: Some("expected after %s"),
    rejected_after: Some("rejected after %s"),
    expected_after_single_line: Some("expected after single-line"),
    expected_before_multi_line: Some("expected before multi-line"),
    expected_after_at_least: Some("at least"),
    ..Messages::NONE
  };

  fn checker(target: Target, option: &str) -> Checker {
    Checker::new(target, Expectation::parse(option), MESSAGES)
  }

  #[test]
  fn always_space_wants_exactly_one_space() {
    let space = checker(Target::Space, "always");
    assert_eq!(space.after(At::new("a, b", 1)), None);
    assert_eq!(
      space.after(At::new("a,b", 1)).as_deref(),
      Some("expected after ,")
    );
    assert_eq!(
      space.after(At::new("a,  b", 1)).as_deref(),
      Some("expected after ,")
    );
    assert_eq!(space.after_one_only(At::new("a,  b", 1)), None);
    // Nothing after the index: nothing to check.
    assert_eq!(space.after(At::new("a,", 1)), None);
    assert_eq!(space.before(At::new("a ,b", 2)), None);
    assert_eq!(
      space.before(At::new("a\t ,b", 3)).as_deref(),
      Some("expected before ,")
    );
    assert_eq!(space.before(At::new(",b", 0)), None);
  }

  #[test]
  fn never_rejects_any_whitespace() {
    let never = checker(Target::Space, "never");
    assert_eq!(never.after(At::new("a,b", 1)), None);
    assert_eq!(
      never.after(At::new("a,\nb", 1)).as_deref(),
      Some("rejected after ,")
    );
    assert_eq!(
      never.before(At::new("a\t,b", 2)).as_deref(),
      Some("rejected before ,")
    );
  }

  #[test]
  fn newlines_count_crlf_and_indentation() {
    let newline = checker(Target::Newline, "always");
    assert_eq!(newline.after(At::new("a,\nb", 1)), None);
    assert_eq!(newline.after(At::new("a,\r\nb", 1)), None);
    assert!(newline.after(At::new("a,\n\nb", 1)).is_some());
    assert_eq!(newline.after_one_only(At::new("a,\n  b", 1)), None);
    assert_eq!(
      newline.before_allowing_indentation(At::new("a\n  ,b", 4)),
      None
    );
    assert!(
      newline
        .before_allowing_indentation(At::new("a ,b", 2))
        .is_some()
    );
    // A plain `before` never accepts a newline.
    assert!(newline.before(At::new("a\n,b", 2)).is_some());
  }

  #[test]
  fn single_and_multi_line_look_at_the_line_check_text() {
    let single = checker(Target::Space, "always-single-line");
    assert_eq!(
      single.after(At::new("a,b", 1)).as_deref(),
      Some("expected after single-line")
    );
    assert_eq!(single.after(At::new("a,b", 1).line_check("x\ny")), None);
    // An empty line check text falls back to the source.
    assert!(single.after(At::new("a,b", 1).line_check("")).is_some());
    let multi = checker(Target::Newline, "always-multi-line");
    assert_eq!(multi.before_allowing_indentation(At::new("a,b", 1)), None);
    assert!(
      multi
        .before_allowing_indentation(At::new("a,b", 1).line_check("\n"))
        .is_some()
    );
  }

  #[test]
  fn index_minus_one_checks_what_starts_the_text() {
    let newline = checker(Target::Newline, "always");
    assert_eq!(newline.after_one_only(At::start_of("\n  b")), None);
    assert_eq!(
      newline.after_one_only(At::start_of(" b")).as_deref(),
      Some("expected after ")
    );
  }

  #[test]
  fn the_error_target_replaces_the_character() {
    let space = checker(Target::Space, "always");
    assert_eq!(
      space
        .after(At::new("@media(", 5).err_target("@media"))
        .as_deref(),
      Some("expected after @media")
    );
  }

  #[test]
  fn the_scss_copy_differs_at_the_edges() {
    let scss = checker(Target::Space, "always").scss();
    // The end of the text is not the end of the check.
    assert_eq!(
      scss.after(At::new("$a:", 2)).as_deref(),
      Some("expected after :")
    );
    let at_least = checker(Target::Space, "at-least-one-space").scss();
    assert_eq!(at_least.after(At::new("$a:  1", 2)), None);
    assert!(at_least.after(At::new("$a:1", 2)).is_some());
    let newline = checker(Target::Newline, "always").scss();
    assert_eq!(newline.before(At::new("a\n,b", 2)), None);
  }

  #[test]
  fn fixes_trim_the_whitespace_next_to_the_index() {
    let edits = fix_space_after(Expectation::Always, 10, "a,  \tb", 1);
    assert_eq!(edits, [Edit::new(Span::from_range(12, 15), " ")]);
    let edits = fix_space_before(Expectation::Never, 0, "a \n,b", 3);
    assert_eq!(edits, [Edit::new(Span::from_range(1, 3), "")]);
  }
}
