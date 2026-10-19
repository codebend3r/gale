//! The settings a file is linted with, for the CLI and the language server
//! alike: whether it is ignored, which parser reads it, and its rule set.
//!
//! [`RuleSetResolver`] finds each file's closest config (or the one named on
//! the command line), applies its `ignoreFiles`, `overrides` and
//! `customSyntax`, and builds the file's [`RuleSet`], once per config and set
//! of matching overrides.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gale_css_parser::{Syntax, detect_syntax};
use gale_linter::RuleSet;
use globset::{Glob, GlobMatcher};

use crate::discovery::load_config;
use crate::model::GaleConfig;
use crate::resolver::ConfigResolver;
use crate::rule_set::{RunSettings, WithoutConfig};

/// Where files' configs come from.
pub enum ConfigSource {
  /// One config for every file, as `--config` names it.
  Explicit(GaleConfig),
  /// Each file's closest config file, as Stylelint finds it.  `root` is the
  /// config for the working directory (or the editor's workspace), already
  /// loaded, with its path; a file no config file covers falls back on it.
  Closest { root: Option<(PathBuf, GaleConfig)> },
}

/// What a [`RuleSetResolver`] is built from.
pub struct ResolverOptions {
  /// Where the configs come from.
  pub source: ConfigSource,
  /// The rules for a file when there is no config at all.
  pub without_config: WithoutConfig,
  /// A `customSyntax` that applies to every file, whatever its config says
  /// (the command line's `--custom-syntax`).
  pub forced_syntax: Option<String>,
  /// What the run adds to every config.
  pub run: RunSettings,
  /// The directory relative paths are relative to.
  pub cwd: PathBuf,
}

/// Why a file is not linted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ignored {
  /// Its config's top-level `ignoreFiles` matches it.
  IgnoreFiles,
  /// An override whose `files` match it lists it in its own `ignoreFiles`,
  /// which Stylelint reports as an ignored, clean source.
  Override,
}

/// How one file is linted.
pub struct FileSettings {
  /// The config that applies to the file, which identifies it among the
  /// resolver's configs (see [`RuleSetResolver::configs`]).
  pub config_id: usize,
  /// That config.
  pub config: Arc<GaleConfig>,
  /// Why the file is not linted, if it is not.
  pub ignored: Option<Ignored>,
  /// The parser for the file, or the unsupported `customSyntax` that means
  /// gale skips it.
  pub syntax: Result<Syntax, String>,
  /// The rules the file is linted with.
  pub rules: Arc<RuleSet>,
}

/// A config the resolver has loaded, with its `ignoreFiles` compiled.
struct LoadedConfig {
  config: Arc<GaleConfig>,
  ignore_files: Vec<GlobMatcher>,
}

impl LoadedConfig {
  /// `config`, ready to answer for files.
  fn new(config: GaleConfig) -> Self {
    // Patterns are written relative to the config's directory; a leading
    // `./` adds nothing to that.
    let ignore_files = config
      .ignore_patterns
      .iter()
      .filter_map(|pattern| {
        let pattern = pattern.strip_prefix("./").unwrap_or(pattern);
        Glob::new(pattern).ok().map(|glob| glob.compile_matcher())
      })
      .collect();
    Self {
      config: Arc::new(config),
      ignore_files,
    }
  }

  /// Whether the config's `ignoreFiles` matches the file at `abs`, as a
  /// path relative to the config's directory (or as given, outside it).
  fn ignores(&self, abs: &Path) -> bool {
    if self.ignore_files.is_empty() {
      return false;
    }
    let relative = self
      .config
      .config_dir
      .as_deref()
      .and_then(|dir| abs.strip_prefix(dir).ok())
      .unwrap_or(abs);
    self
      .ignore_files
      .iter()
      .any(|m| m.is_match(relative) || m.is_match(abs))
  }
}

/// Answers, for any file, how it is linted (see [`FileSettings`]), loading
/// each config and building each distinct rule set once.
pub struct RuleSetResolver {
  /// Whether one config applies to every file (`--config`).
  explicit: bool,
  without_config: WithoutConfig,
  forced_syntax: Option<String>,
  run: RunSettings,
  cwd: PathBuf,
  /// Which config file applies to each directory.
  dirs: ConfigResolver,
  configs: Vec<LoadedConfig>,
  /// Position in `configs` of each config file loaded.
  by_path: HashMap<PathBuf, usize>,
  /// Position in `configs` of the config for files no config file covers
  /// (and of the explicit config, which covers every file).
  fallback: Option<usize>,
  /// The rule set for each config and list of matching overrides.
  rule_sets: HashMap<(usize, Vec<usize>), Arc<RuleSet>>,
}

impl RuleSetResolver {
  /// A resolver with nothing loaded but the config `options` hands it.
  pub fn new(options: ResolverOptions) -> Self {
    let ResolverOptions {
      source,
      without_config,
      forced_syntax,
      run,
      cwd,
    } = options;
    let mut resolver = Self {
      explicit: matches!(source, ConfigSource::Explicit(_)),
      without_config,
      forced_syntax,
      run,
      cwd,
      dirs: ConfigResolver::new(),
      configs: Vec::new(),
      by_path: HashMap::new(),
      fallback: None,
      rule_sets: HashMap::new(),
    };
    match source {
      ConfigSource::Explicit(config) => resolver.fallback = Some(resolver.add(config)),
      ConfigSource::Closest {
        root: Some((path, config)),
      } => {
        let id = resolver.add(config);
        resolver.by_path.insert(path, id);
        resolver.fallback = Some(id);
      }
      ConfigSource::Closest { root: None } => {}
    }
    resolver
  }

  /// Keep `config` and return its id.
  fn add(&mut self, config: GaleConfig) -> usize {
    self.configs.push(LoadedConfig::new(config));
    self.configs.len() - 1
  }

  /// Every config loaded so far, by id: the root (or explicit) config
  /// first, then each one a file needed.
  pub fn configs(&self) -> impl Iterator<Item = &Arc<GaleConfig>> {
    self.configs.iter().map(|loaded| &loaded.config)
  }

  /// `path` made absolute against the working directory.
  fn absolute(&self, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
      path.to_path_buf()
    } else {
      self.cwd.join(path)
    }
  }

  /// The id of the config that applies to the file at `abs`, loading it the
  /// first time.  A config file that fails to load is warned about and
  /// counts as empty, as it does for the root config's neighbours.
  fn config_for(&mut self, abs: &Path) -> usize {
    if self.explicit {
      return self.fallback_id();
    }
    let dir = abs.parent().unwrap_or(Path::new(".")).to_path_buf();
    let Some(config_path) = self.dirs.find_config_path_for_dir(dir) else {
      return self.fallback_id();
    };
    if let Some(&id) = self.by_path.get(&config_path) {
      return id;
    }
    let config = load_config(&config_path).unwrap_or_else(|err| {
      eprintln!(
        "Warning: failed to load config {}: {err}",
        config_path.display()
      );
      GaleConfig::default()
    });
    let id = self.add(config);
    self.by_path.insert(config_path, id);
    id
  }

  /// The id of the config for files no config file covers: the root config
  /// when there is one, else the stand-in for no config.
  fn fallback_id(&mut self) -> usize {
    match self.fallback {
      Some(id) => id,
      None => {
        let id = self.add(self.without_config.config());
        self.fallback = Some(id);
        id
      }
    }
  }

  /// How the file at `path` (as the user named it, relative to the working
  /// directory or absolute) is linted.
  pub fn resolve(&mut self, path: &str) -> FileSettings {
    let abs = self.absolute(path);
    let config_id = self.config_for(&abs);
    let loaded = &self.configs[config_id];
    let config = Arc::clone(&loaded.config);

    let ignored = if loaded.ignores(&abs) {
      Some(Ignored::IgnoreFiles)
    } else if config.is_file_ignored_by_override(path) {
      Some(Ignored::Override)
    } else {
      None
    };
    let syntax = syntax_for_file(self.forced_syntax.as_deref(), &config, path);

    let overrides = config.matching_overrides(path);
    let run = &self.run;
    let rules = Arc::clone(
      self
        .rule_sets
        .entry((config_id, overrides))
        .or_insert_with_key(|(_, overrides)| Arc::new(config.rule_set(overrides, run))),
    );

    FileSettings {
      config_id,
      config,
      ignored,
      syntax,
      rules,
    }
  }
}

/// Map a Stylelint `customSyntax` package name onto the parser gale uses.
///
/// Returns `None` for syntaxes gale cannot parse (`postcss-markdown`, ...),
/// and for `postcss-html`, which [`syntax_for_file`] accepts on its own: an
/// HTML-like file's style blocks each pick their parser from their `lang`.
pub fn syntax_for_custom_syntax(name: &str) -> Option<Syntax> {
  match name.to_ascii_lowercase().as_str() {
    "postcss" => Some(Syntax::Css),
    "postcss-scss" => Some(Syntax::Scss),
    "postcss-less" => Some(Syntax::Less),
    "postcss-sass" => Some(Syntax::Sass),
    _ => None,
  }
}

/// Whether a `customSyntax` names postcss-html, the syntax Stylelint lints
/// HTML-like files with.
fn is_postcss_html(custom_syntax: &str) -> bool {
  custom_syntax.eq_ignore_ascii_case("postcss-html")
}

/// The parser to use for `file_path`, or `Err` with the name of the
/// `customSyntax` gale cannot parse, when the file is to be skipped.
///
/// A forced syntax (`--custom-syntax`) applies to every file and picks the
/// parser directly.  The config's own `customSyntax` only decides whether a
/// file is skipped: supported values still parse by file extension.
/// `postcss-html` parses by extension too: an HTML-like file is read for its
/// style blocks whatever the syntax says, and other files are style sheets.
pub fn syntax_for_file(
  forced: Option<&str>,
  config: &GaleConfig,
  file_path: &str,
) -> Result<Syntax, String> {
  if let Some(name) = forced {
    if is_postcss_html(name) {
      return Ok(detect_syntax(file_path));
    }
    return syntax_for_custom_syntax(name).ok_or_else(|| name.to_string());
  }

  if let Some(name) = config.custom_syntax_for_file(file_path)
    && !is_postcss_html(name)
    && syntax_for_custom_syntax(name).is_none()
  {
    return Err(name.to_string());
  }

  Ok(detect_syntax(file_path))
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_linter::DisableOptions;

  /// A project directory holding `files`, each a relative path and its text.
  fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for (path, text) in files {
      let path = tmp.path().join(path);
      std::fs::create_dir_all(path.parent().unwrap()).unwrap();
      std::fs::write(path, text).unwrap();
    }
    tmp
  }

  /// A resolver for files under `dir`, rooted at the config found there.
  fn closest(dir: &Path, without_config: WithoutConfig) -> RuleSetResolver {
    let root = crate::find_config(dir).map(|path| {
      let config = load_config(&path).unwrap();
      (path, config)
    });
    RuleSetResolver::new(ResolverOptions {
      source: ConfigSource::Closest { root },
      without_config,
      forced_syntax: None,
      run: RunSettings::default(),
      cwd: dir.to_path_buf(),
    })
  }

  /// The rules `settings` reports on `source`, sorted.
  fn rules_on(settings: &FileSettings, source: &str, path: &str) -> Vec<String> {
    let mut rules: Vec<String> = settings
      .rules
      .lint(source, path, settings.syntax.clone().unwrap())
      .diagnostics
      .into_iter()
      .map(|d| d.rule_name)
      .collect();
    rules.sort();
    rules
  }

  #[test]
  fn each_file_gets_its_closest_config() {
    let tmp = project(&[
      (
        ".stylelintrc.json",
        r#"{ "rules": { "block-no-empty": true } }"#,
      ),
      (
        "nested/.stylelintrc.json",
        r#"{ "rules": { "color-hex-case": "lower" } }"#,
      ),
    ]);
    let mut resolver = closest(tmp.path(), WithoutConfig::NoRules);
    let source = "a {}\nb { color: #FFF; }\n";
    let root = resolver.resolve("a.css");
    assert_eq!(rules_on(&root, source, "a.css"), ["block-no-empty"]);
    let nested = resolver.resolve("nested/a.css");
    assert_eq!(
      rules_on(&nested, source, "nested/a.css"),
      ["color-hex-case"]
    );
    assert_ne!(root.config_id, nested.config_id);
    assert_eq!(resolver.configs().count(), 2);
  }

  #[test]
  fn rule_sets_are_shared_by_config_and_matching_overrides() {
    let tmp = project(&[(
      ".stylelintrc.json",
      r#"{
        "rules": { "block-no-empty": true },
        "overrides": [{ "files": ["**/*.scss"], "rules": { "color-hex-case": "lower" } }]
      }"#,
    )]);
    let mut resolver = closest(tmp.path(), WithoutConfig::NoRules);
    let a = resolver.resolve("a.css");
    let b = resolver.resolve("sub/b.css");
    let c = resolver.resolve("c.scss");
    assert!(Arc::ptr_eq(&a.rules, &b.rules));
    assert!(!Arc::ptr_eq(&a.rules, &c.rules));
    assert_eq!(
      rules_on(&c, "a {}\nb { color: #FFF; }\n", "c.scss"),
      ["block-no-empty", "color-hex-case"]
    );
  }

  #[test]
  fn ignore_files_are_relative_to_the_config() {
    let tmp = project(&[
      (
        ".stylelintrc.json",
        r#"{ "ignoreFiles": ["vendor/**"], "rules": {} }"#,
      ),
      (
        "pkg/.stylelintrc.json",
        r#"{ "ignoreFiles": ["./generated/*.css"], "rules": {},
             "overrides": [{ "files": ["legacy/**"], "ignoreFiles": ["legacy/old.css"] }] }"#,
      ),
    ]);
    let mut resolver = closest(tmp.path(), WithoutConfig::NoRules);
    // Override globs are matched relative to the config, which needs the
    // absolute path when the process runs elsewhere, as tests do.
    let ignored = |resolver: &mut RuleSetResolver, path: &str| {
      let abs = tmp.path().join(path);
      resolver.resolve(&abs.display().to_string()).ignored
    };
    assert_eq!(
      ignored(&mut resolver, "vendor/a.css"),
      Some(Ignored::IgnoreFiles)
    );
    assert_eq!(ignored(&mut resolver, "a.css"), None);
    assert_eq!(
      ignored(&mut resolver, "pkg/generated/a.css"),
      Some(Ignored::IgnoreFiles)
    );
    assert_eq!(ignored(&mut resolver, "generated/a.css"), None);
    assert_eq!(
      ignored(&mut resolver, "pkg/legacy/old.css"),
      Some(Ignored::Override)
    );
    assert_eq!(ignored(&mut resolver, "pkg/legacy/new.css"), None);
  }

  #[test]
  fn files_no_config_covers_fall_back_on_the_root_then_the_stand_in() {
    let tmp = project(&[("a.css", "a {}")]);
    let mut none = closest(tmp.path(), WithoutConfig::NoRules);
    assert!(rules_on(&none.resolve("a.css"), "a {}", "a.css").is_empty());
    let mut all = closest(tmp.path(), WithoutConfig::AllRules);
    assert!(rules_on(&all.resolve("a.css"), "a {}", "a.css").contains(&"block-no-empty".into()));

    // An explicit config applies everywhere, nested configs or not.
    let tmp = project(&[(
      "nested/.stylelintrc.json",
      r#"{ "rules": { "color-hex-case": "lower" } }"#,
    )]);
    let explicit = GaleConfig {
      rules: HashMap::from([(
        "block-no-empty".to_string(),
        crate::RuleConfig {
          severity: None,
          options: None,
        },
      )]),
      ..GaleConfig::default()
    };
    let mut resolver = RuleSetResolver::new(ResolverOptions {
      source: ConfigSource::Explicit(explicit),
      without_config: WithoutConfig::NoRules,
      forced_syntax: None,
      run: RunSettings::default(),
      cwd: tmp.path().to_path_buf(),
    });
    let settings = resolver.resolve("nested/a.css");
    assert_eq!(
      rules_on(&settings, "a {}\nb { color: #FFF; }\n", "nested/a.css"),
      ["block-no-empty"]
    );
  }

  #[test]
  fn the_run_settings_reach_every_rule_set() {
    let tmp = project(&[(
      ".stylelintrc.json",
      r#"{ "reportNeedlessDisables": true, "rules": { "block-no-empty": true } }"#,
    )]);
    let root = crate::find_config(tmp.path()).map(|p| (p.clone(), load_config(&p).unwrap()));
    let mut resolver = RuleSetResolver::new(ResolverOptions {
      source: ConfigSource::Closest { root },
      without_config: WithoutConfig::NoRules,
      forced_syntax: None,
      run: RunSettings {
        disables: DisableOptions {
          ignore: true,
          ..DisableOptions::default()
        },
        ..RunSettings::default()
      },
      cwd: tmp.path().to_path_buf(),
    });
    let disables = resolver.resolve("a.css").rules.disables();
    assert!(disables.ignore && disables.report_needless);
  }

  #[test]
  fn custom_syntax_names_map_to_parsers() {
    assert_eq!(syntax_for_custom_syntax("postcss"), Some(Syntax::Css));
    assert_eq!(syntax_for_custom_syntax("postcss-scss"), Some(Syntax::Scss));
    assert_eq!(syntax_for_custom_syntax("PostCSS-Less"), Some(Syntax::Less));
    assert_eq!(syntax_for_custom_syntax("postcss-sass"), Some(Syntax::Sass));
    assert_eq!(syntax_for_custom_syntax("postcss-html"), None);
    assert_eq!(syntax_for_custom_syntax("postcss-markdown"), None);
  }

  #[test]
  fn syntax_is_detected_from_the_extension_by_default() {
    let config = GaleConfig::default();
    assert_eq!(syntax_for_file(None, &config, "a.css"), Ok(Syntax::Css));
    assert_eq!(syntax_for_file(None, &config, "a.scss"), Ok(Syntax::Scss));
  }

  #[test]
  fn a_forced_syntax_picks_the_parser() {
    let config = GaleConfig::default();
    assert_eq!(
      syntax_for_file(Some("postcss-scss"), &config, "a.css"),
      Ok(Syntax::Scss)
    );
    // An unsupported syntax skips the file rather than parsing it.
    assert_eq!(
      syntax_for_file(Some("postcss-markdown"), &config, "a.css"),
      Err("postcss-markdown".to_string())
    );
    // postcss-html parses by extension: style blocks pick their own parser.
    assert_eq!(
      syntax_for_file(Some("postcss-html"), &config, "a.vue"),
      Ok(Syntax::Css)
    );
    assert_eq!(
      syntax_for_file(Some("postcss-html"), &config, "a.scss"),
      Ok(Syntax::Scss)
    );
  }

  #[test]
  fn postcss_html_from_the_config_is_accepted() {
    let config = GaleConfig {
      custom_syntax: Some("postcss-html".to_string()),
      ..Default::default()
    };
    assert_eq!(syntax_for_file(None, &config, "a.vue"), Ok(Syntax::Css));
    assert_eq!(syntax_for_file(None, &config, "a.svelte"), Ok(Syntax::Css));
  }

  #[test]
  fn files_whose_config_syntax_is_unsupported_are_skipped() {
    let config = GaleConfig {
      custom_syntax: Some("postcss-markdown".to_string()),
      ..Default::default()
    };
    assert_eq!(
      syntax_for_file(None, &config, "a.css"),
      Err("postcss-markdown".to_string())
    );
    // A forced syntax overrides the config's customSyntax entirely.
    assert_eq!(
      syntax_for_file(Some("postcss-less"), &config, "a.css"),
      Ok(Syntax::Less)
    );
  }

  #[test]
  fn a_supported_config_syntax_keeps_extension_detection() {
    // A supported config `customSyntax` decides only that the file is not
    // skipped; the parser still comes from the extension.
    let config = GaleConfig {
      custom_syntax: Some("postcss-scss".to_string()),
      ..Default::default()
    };
    assert_eq!(syntax_for_file(None, &config, "a.css"), Ok(Syntax::Css));
  }
}
