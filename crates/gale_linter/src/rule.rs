use std::rc::Rc;

use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Severity};

use crate::file_cache::FileCache;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};

/// Context passed to each rule when checking a file.
pub struct RuleContext<'a> {
  /// The path to the file being linted.
  pub file_path: &'a str,
  /// The full source text of the file.
  pub source: &'a str,
  /// The CSS syntax variant.
  pub syntax: Syntax,
  /// What the rules linting this file share, built once per file.  The
  /// runner always sets it; without one (a rule's own tests, say) every
  /// accessor such as [`Self::postcss_tree`] builds afresh.
  pub cache: Option<&'a FileCache<'a>>,
  /// What the run knows about the project beyond its config, such as the
  /// installed Stylelint version.
  pub env: LintEnv,
  /// The host metadata for an embedded sheet; absent for standalone sources.
  pub embedded: Option<&'a crate::embedded::EmbeddedRoot>,
}

impl<'a> RuleContext<'a> {
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
  /// Whether the primary option is an object, such as a map from property
  /// names to lists, which decides whether a bare `{...}` setting is the
  /// primary option or, as gale reads it for other rules, the secondary
  /// options (see [`Setting`]).
  pub primary_option_object: bool,
}

/// A rule configured with its options, ready to check files.
///
/// A rule set configures each of its rules once, and every file linted with
/// the set is checked by the same configured rule.  The runner gives each
/// problem its severity, so a check leaves it at the default.
pub trait Check: Send + Sync {
  /// Check the file through its statement tree, the tree a Stylelint rule
  /// reads as `root` (see [`PostcssTree`]).
  ///
  /// The runner calls it once per file with the tree every rule shares.
  fn check(&self, tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    let _ = (tree, context);
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
/// primary option can be an array (Stylelint's `rule.primaryOptionArray`),
/// and `primary_option_object: true` one whose primary option is an object.
macro_rules! declare_rule {
  (
    $(#[$attr:meta])*
    $vis:vis struct $rule:ident => $check:ty {
      name: $name:expr,
      description: $description:expr,
      default_severity: $severity:ident
      $(, primary_option_array: $array:expr)?
      $(, primary_option_object: $object:expr)?
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
        primary_option_object: false $(|| $object)?,
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

#[cfg(test)]
mod tests {
  use super::*;

  /// A context over `source`.
  fn context(source: &str, syntax: Syntax) -> RuleContext<'_> {
    RuleContext {
      env: Default::default(),
      file_path: "test.css",
      source,
      syntax,
      cache: None,
      embedded: None,
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

    let other = source.clone();
    let elsewhere = RuleContext {
      env: Default::default(),
      cache: Some(&cache),
      ..context(&other, Syntax::Css)
    };
    let tree = elsewhere.postcss_tree();
    assert!(!Rc::ptr_eq(&tree, &cache.postcss_tree()));
    assert!(std::ptr::eq(tree.source(), other.as_str()));

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
    fn check(&self, _tree: &PostcssTree, _context: &RuleContext) -> Vec<Diagnostic> {
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
    let setting = Setting::new(&meta, Some(&options));
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
        primary_option_object: false,
      }
    );
    let ctx = context("", Syntax::Css);
    let check = configure(&WordRule, serde_json::json!("b")).ok().unwrap();
    assert_eq!(check.check(&ctx.postcss_tree(), &ctx)[0].message, "b");
    assert_eq!(
      configure(&WordRule, serde_json::json!("c")).err().unwrap(),
      ["Invalid option value \"c\" for rule \"test/word\""]
    );
  }
}
