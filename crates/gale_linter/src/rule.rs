use std::rc::Rc;

use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity};

use crate::file_cache::FileCache;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
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
  /// LEGACY: the rule's options exactly as the config gave them, for rules
  /// not yet converted to typed options ([`LegacyRule`]).  A converted rule
  /// reads its options once, in its `from_setting`, and never looks here.
  /// Goes away with [`LegacyRule`].
  pub options: Option<&'a serde_json::Value>,
  /// What the rules linting this file share, built once per file.  The
  /// runner always sets it; without one (a rule's own tests, say) every
  /// accessor such as [`Self::postcss_tree`] builds afresh.
  pub cache: Option<&'a FileCache<'a>>,
  /// What the run knows about the project beyond its config, such as the
  /// installed Stylelint version.
  pub env: LintEnv,
}

impl<'a> RuleContext<'a> {
  /// LEGACY (see [`RuleContext::options`]): extract the **primary option**
  /// from Stylelint-format options.
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

  /// LEGACY (see [`RuleContext::options`]): extract the primary option as a
  /// `&str`, handling both plain strings
  /// and array-format options `["value", { ... }]`.
  pub fn primary_option_str(&self) -> Option<&'a str> {
    self.primary_option().and_then(|v| v.as_str())
  }

  /// LEGACY (see [`RuleContext::options`]): extract the **secondary options
  /// object** from Stylelint-format options.
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

/// What the registry knows about a rule before it is configured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleMeta {
  /// Unique rule name, e.g. "block-no-empty".
  pub name: &'static str,
  /// Human-readable description of what this rule checks.
  pub description: &'static str,
  /// The severity of the rule's problems when neither its config nor the
  /// config's `defaultSeverity` gives one.
  pub default_severity: Severity,
  /// Stylelint's `primaryOptionArray`: whether the primary option can be an
  /// array, which decides how a setting written as an array is split into
  /// primary and secondary options (see [`Setting::new`]).
  pub primary_option_array: bool,
}

/// A rule configured with its options, ready to check files.
///
/// A rule set configures each of its rules once, and every file linted with
/// the set is checked by the same configured rule.  The runner gives each
/// problem its severity, so a check leaves it at the default.
pub trait Check: Send + Sync {
  /// Check a single CSS node and return any diagnostics.
  fn check(&self, node: &CssNode, context: &RuleContext) -> Vec<Diagnostic> {
    let _ = (node, context);
    vec![]
  }

  /// Check the entire document (all top-level nodes). Used for rules that need
  /// document-level context like duplicate detection or source-level checks.
  fn check_root(&self, nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let _ = (nodes, context);
    vec![]
  }
}

/// A lint rule, as the registry holds it: its metadata, and a way to
/// configure it.
///
/// A rule file declares its rule with [`declare_rule!`], which implements
/// this for a unit struct from a type that holds the parsed options,
/// implements [`Check`] and has a `from_setting` constructor.
pub trait Rule: Send + Sync {
  /// The rule's name, description, default severity and option shape.
  fn meta(&self) -> RuleMeta;

  /// The rule configured with `setting`, or every problem with its options.
  /// A rule with invalid options does not run.
  fn configure(
    &self,
    setting: &Setting,
    env: &LintEnv,
  ) -> Result<Box<dyn Check + '_>, Vec<InvalidOption>>;

  /// Unique rule name, e.g. "block-no-empty".
  fn name(&self) -> &'static str {
    self.meta().name
  }

  /// Human-readable description of what this rule checks.
  fn description(&self) -> &'static str {
    self.meta().description
  }

  /// The default severity for diagnostics produced by this rule.
  fn default_severity(&self) -> Severity {
    self.meta().default_severity
  }

  /// LEGACY: whether the rule is a [`LegacyRule`], which reads its options
  /// from the context on every node and sets its own severity.
  #[doc(hidden)]
  fn is_legacy(&self) -> bool {
    false
  }
}

/// Declare a rule whose options are parsed once: a unit struct the registry
/// holds, implementing [`Rule`] by building `$check` with its
/// `fn from_setting(&Setting, &LintEnv) -> Result<Self, Vec<InvalidOption>>`.
///
/// ```ignore
/// declare_rule! {
///   /// Disallow empty blocks.
///   pub struct BlockNoEmpty => BlockNoEmptyCheck {
///     name: NAME,
///     description: "Disallow empty blocks",
///     default_severity: Warning,
///   }
/// }
/// ```
///
/// `primary_option_array: true` after the severity marks a rule whose
/// primary option can be an array (Stylelint's `rule.primaryOptionArray`).
macro_rules! declare_rule {
  (
    $(#[$attr:meta])*
    $vis:vis struct $rule:ident => $check:ty {
      name: $name:expr,
      description: $description:expr,
      default_severity: $severity:ident
      $(, primary_option_array: $array:expr)?
      $(,)?
    }
  ) => {
    $(#[$attr])*
    $vis struct $rule;

    impl $rule {
      /// The rule's name, description, default severity and option shape.
      pub const META: $crate::rule::RuleMeta = $crate::rule::RuleMeta {
        name: $name,
        description: $description,
        default_severity: gale_diagnostics::Severity::$severity,
        primary_option_array: false $(|| $array)?,
      };
    }

    impl $crate::rule::Rule for $rule {
      fn meta(&self) -> $crate::rule::RuleMeta {
        Self::META
      }

      fn configure(
        &self,
        setting: &$crate::options::Setting,
        env: &$crate::lint_env::LintEnv,
      ) -> Result<Box<dyn $crate::rule::Check + '_>, Vec<$crate::options::InvalidOption>> {
        Ok(Box::new(<$check>::from_setting(setting, env)?))
      }
    }
  };
}
pub(crate) use declare_rule;

/// LEGACY: a rule that has not been converted to typed options yet.  It
/// reads its raw options from [`RuleContext::options`] on every node and
/// sets each diagnostic's severity itself.  Every legacy rule is a [`Rule`]
/// whose configuration always succeeds.  Rule files import it as `Rule`
/// (`use crate::rule::{LegacyRule as Rule, ...}`) so they compile unchanged;
/// it goes away once every rule is converted.
pub trait LegacyRule: Send + Sync {
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

impl<T: LegacyRule> Rule for T {
  fn meta(&self) -> RuleMeta {
    RuleMeta {
      name: LegacyRule::name(self),
      description: LegacyRule::description(self),
      default_severity: LegacyRule::default_severity(self),
      primary_option_array: false,
    }
  }

  /// A legacy rule takes any options; it reads them as it checks.
  fn configure(
    &self,
    _setting: &Setting,
    _env: &LintEnv,
  ) -> Result<Box<dyn Check + '_>, Vec<InvalidOption>> {
    Ok(Box::new(LegacyCheck(self)))
  }

  fn name(&self) -> &'static str {
    LegacyRule::name(self)
  }

  fn description(&self) -> &'static str {
    LegacyRule::description(self)
  }

  fn default_severity(&self) -> Severity {
    LegacyRule::default_severity(self)
  }

  fn is_legacy(&self) -> bool {
    true
  }
}

/// LEGACY: a [`LegacyRule`] as a [`Check`], reading its options from the
/// context the runner fills in.
struct LegacyCheck<'r, T>(&'r T);

impl<T: LegacyRule> Check for LegacyCheck<'_, T> {
  fn check(&self, node: &CssNode, context: &RuleContext) -> Vec<Diagnostic> {
    self.0.check(node, context)
  }

  fn check_root(&self, nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    self.0.check_root(nodes, context)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A context over `source` with no options.
  fn context(source: &str, syntax: Syntax) -> RuleContext<'_> {
    RuleContext {
      env: Default::default(),
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
      env: Default::default(),
      cache: Some(&cache),
      ..context(&source, Syntax::Css)
    };
    assert!(Rc::ptr_eq(&ctx.postcss_tree(), &cache.postcss_tree()));
    assert!(Rc::ptr_eq(&ctx.scanned_rules(), &cache.scanned_rules()));

    let other = source.clone();
    let elsewhere = RuleContext {
      env: Default::default(),
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

  /// A rule declared the way rule files declare theirs: it reports its
  /// primary option at the root.
  struct ReportsWord {
    word: &'static str,
  }

  impl ReportsWord {
    fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
      let mut options = setting.options();
      let word = options.primary_one_of(&["a", "b"]).unwrap_or("a");
      options.finish()?;
      Ok(Self { word })
    }
  }

  impl Check for ReportsWord {
    fn check_root(&self, _nodes: &[CssNode], _context: &RuleContext) -> Vec<Diagnostic> {
      vec![Diagnostic::new("test/word", self.word)]
    }
  }

  declare_rule! {
    /// Reports its primary option.
    struct WordRule => ReportsWord {
      name: "test/word",
      description: "Reports a word",
      default_severity: Error,
    }
  }

  fn configure(
    rule: &dyn Rule,
    options: serde_json::Value,
  ) -> Result<Box<dyn Check + '_>, Vec<String>> {
    let meta = rule.meta();
    let setting = Setting::new(meta.name, Some(&options), meta.primary_option_array);
    rule
      .configure(&setting, &LintEnv::default())
      .map_err(|problems| problems.into_iter().map(InvalidOption::into_text).collect())
  }

  #[test]
  fn a_declared_rule_is_configured_from_its_setting() {
    assert_eq!(
      WordRule.meta(),
      RuleMeta {
        name: "test/word",
        description: "Reports a word",
        default_severity: Severity::Error,
        primary_option_array: false,
      }
    );
    assert!(!WordRule.is_legacy());
    let ctx = context("", Syntax::Css);
    let check = configure(&WordRule, serde_json::json!("b")).ok().unwrap();
    assert_eq!(check.check_root(&[], &ctx)[0].message, "b");
    assert!(
      check
        .check(&CssNode::Style(Default::default()), &ctx)
        .is_empty()
    );
    assert_eq!(
      configure(&WordRule, serde_json::json!("c")).err().unwrap(),
      ["Invalid option value \"c\" for rule \"test/word\""]
    );
  }

  /// A legacy rule, which reads its options from the context.
  struct ReportsOptions;

  impl LegacyRule for ReportsOptions {
    fn name(&self) -> &'static str {
      "test/legacy"
    }

    fn description(&self) -> &'static str {
      "Reports its options"
    }

    fn default_severity(&self) -> Severity {
      Severity::Warning
    }

    fn check(&self, _node: &CssNode, context: &RuleContext) -> Vec<Diagnostic> {
      vec![Diagnostic::new(
        "test/legacy",
        format!("{:?}", context.options),
      )]
    }
  }

  #[test]
  fn a_legacy_rule_takes_any_options_and_reads_them_as_it_checks() {
    assert!(ReportsOptions.is_legacy());
    assert_eq!(Rule::name(&ReportsOptions), "test/legacy");
    assert_eq!(Rule::default_severity(&ReportsOptions), Severity::Warning);
    let check = configure(&ReportsOptions, serde_json::json!("anything"))
      .ok()
      .unwrap();
    let options = serde_json::json!(3);
    let ctx = RuleContext {
      options: Some(&options),
      ..context("", Syntax::Css)
    };
    let found = check.check(&CssNode::Style(Default::default()), &ctx);
    assert_eq!(found[0].message, "Some(Number(3))");
  }
}
