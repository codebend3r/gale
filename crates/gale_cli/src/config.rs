//! The configuration a run uses: the root config, the switches the
//! command line and the config file share, and the resolver every file's
//! settings come from.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use tracing::debug;

use gale_config::{
  ConfigSource, FixMode, GaleConfig, ResolverOptions, RuleSetResolver, RunSettings, SkippedRules,
  WithoutConfig, unsupported_plugin_warnings,
};
use gale_linter::{DisableOptions, LintEnv};

use crate::args::Cli;
use crate::cache::{CacheStrategy, resolve_cache_path};
use crate::exit::{EXIT_FATAL, EXIT_INVALID_CONFIG, Exit};
use crate::report::choose_formatter;

/// The config for the working directory: the `--config` file, else the
/// closest config file.
pub(crate) struct RootConfig {
  pub(crate) config: GaleConfig,
  /// Where the closest config was found, which files without a closer
  /// config fall back on; `None` with `--config`, or with no config file.
  pub(crate) path: Option<PathBuf>,
}

impl RootConfig {
  /// Load the root config.  One that exists but cannot be loaded is a
  /// configuration error (exit 78), not something to paper over with
  /// defaults.
  pub(crate) fn load(cli: &Cli) -> Result<Self, Exit> {
    let load = |path: &Path| {
      gale_config::load_config(path).map_err(|err| {
        Exit::new(
          EXIT_INVALID_CONFIG,
          format!("Error: failed to load config {}: {err}", path.display()),
        )
      })
    };
    if let Some(path) = &cli.config {
      debug!("Using config file: {}", path.display());
      return Ok(Self {
        config: load(path)?,
        path: None,
      });
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    Ok(match gale_config::find_config(&cwd) {
      Some(path) => Self {
        config: load(&path)?,
        path: Some(path),
      },
      None => Self {
        config: GaleConfig::default(),
        path: None,
      },
    })
  }

  /// Warn about the plugins the config loads that gale does not know.
  pub(crate) fn warn_unsupported_plugins(&self) {
    for warning in unsupported_plugin_warnings(&self.config) {
      eprintln!("{warning}");
    }
  }

  /// The resolver each file's settings come from: its closest config (or
  /// the `--config` file) with the overrides that match it, `forced_syntax`
  /// for every file, and `run` added to every config.
  ///
  /// `without_config` is what a file gets when there is no config file at
  /// all.
  pub(crate) fn resolver(
    &self,
    cli: &Cli,
    without_config: WithoutConfig,
    forced_syntax: Option<String>,
    run: RunSettings,
  ) -> RuleSetResolver {
    let source = if cli.config.is_some() {
      ConfigSource::Explicit(self.config.clone())
    } else {
      ConfigSource::Closest {
        root: self.path.clone().map(|path| (path, self.config.clone())),
      }
    };
    RuleSetResolver::new(ResolverOptions {
      source,
      without_config,
      forced_syntax,
      run,
      cwd: std::env::current_dir().unwrap_or_default(),
    })
  }
}

/// The switches that can come from the command line or the config file.
/// A flag on the command line wins; otherwise the config decides, matching
/// Stylelint where unspecified CLI flags fall back to config properties.
pub(crate) struct Settings {
  /// The formatter's name.
  pub(crate) formatter: String,
  pub(crate) quiet: bool,
  pub(crate) use_cache: bool,
  pub(crate) allow_empty_input: bool,
  pub(crate) cache_path: PathBuf,
  pub(crate) cache_strategy: CacheStrategy,
  pub(crate) fix: Option<FixMode>,
}

impl Settings {
  /// The settings for `cli` and the root `config`.
  ///
  /// An unknown formatter in the config is a configuration error, as an
  /// unknown `--formatter` is a usage error; neither falls back to text.
  /// An unknown cache strategy is fatal, as in Stylelint.
  pub(crate) fn new(cli: &Cli, config: &GaleConfig) -> Result<Self, Exit> {
    let formatter = choose_formatter(cli.formatter.as_deref(), config.formatter.as_deref())
      .map_err(|message| Exit::new(EXIT_INVALID_CONFIG, format!("Error: {message}")))?;

    // Gale never emits deprecation warnings, so the flag only needs accepting.
    if cli.quiet_deprecation_warnings {
      debug!("--quiet-deprecation-warnings has nothing to silence in Gale");
    }

    let cache_strategy = match cli
      .cache_strategy
      .as_deref()
      .or(config.cache_strategy.as_deref())
    {
      None => CacheStrategy::default(),
      Some(name) => CacheStrategy::parse(name).ok_or_else(|| {
        Exit::new(
          EXIT_FATAL,
          format!(
            "\"{name}\" cache strategy is unsupported. Specify either \"metadata\" or \"content\""
          ),
        )
      })?,
    };
    let cache_location = cli
      .cache_location
      .clone()
      .or_else(|| config.cache_location.clone());

    // `--fix` takes `lax`; any other mode is strict, the default.
    let fix = match cli.fix.as_deref() {
      Some("lax") => Some(FixMode::Lax),
      Some(_) => Some(FixMode::Strict),
      None => config.fix,
    };

    Ok(Self {
      formatter,
      quiet: cli.quiet || config.quiet,
      use_cache: cli.cache || config.cache,
      allow_empty_input: cli.allow_empty_input || config.allow_empty_input,
      cache_path: resolve_cache_path(cache_location.as_deref()),
      cache_strategy,
      fix,
    })
  }
}

/// What the run adds to every config's rule sets: the command line's
/// disable switches, and the Stylelint installed for the project in the
/// working directory, whose version a few rules follow.
pub(crate) fn run_settings(cli: &Cli) -> RunSettings {
  RunSettings {
    disables: DisableOptions {
      ignore: cli.ignore_disables,
      report_needless: cli.report_needless_disables,
      report_invalid_scope: cli.report_invalid_scope_disables,
      report_descriptionless: cli.report_descriptionless_disables,
      report_unscoped: cli.report_unscoped_disables,
    },
    env: LintEnv::detect(&std::env::current_dir().unwrap_or_default()),
  }
}

/// Warn once about the rules that the configs the resolver loaded ask for
/// and gale skips: real Stylelint and plugin rules gale has not
/// implemented, and rules Stylelint has removed.  Names that are no rule at
/// all are reported in each file as `Unknown rule`, as Stylelint does.
pub(crate) fn warn_skipped_rules(resolver: &RuleSetResolver) {
  let mut skipped = SkippedRules::default();
  for config in resolver.configs() {
    skipped.extend(SkippedRules::in_config(config));
  }
  for warning in skipped.warnings() {
    eprintln!("{warning}");
  }
}

/// `--print-config`: print the rules that apply to `file` and the ignore
/// patterns of its config, as JSON.
pub(crate) fn print_config(cli: &Cli, root: &RootConfig, file: &Path) -> Result<()> {
  let file_str = file.display().to_string();
  let mut resolver = root.resolver(cli, WithoutConfig::NoRules, None, RunSettings::default());
  let config = resolver.resolve(&file_str).config;
  let mut rules_json = serde_json::Map::new();
  for (name, rc) in &config.rules_for_file(&file_str) {
    let severity = rc
      .severity
      .as_ref()
      .map(|s| format!("{s:?}").to_lowercase())
      .unwrap_or_else(|| "error".to_string());
    // Show [severity, options] for rules with options.  A setting written
    // `[true, setting]` prints as `["error", setting]`, which means the same.
    let options = match &rc.options {
      Some(serde_json::Value::Array(items))
        if items.first() == Some(&serde_json::Value::Bool(true)) =>
      {
        items.get(1)
      }
      other => other.as_ref(),
    };
    if let Some(opts) = options {
      rules_json.insert(name.clone(), serde_json::json!([severity, opts]));
    } else {
      rules_json.insert(name.clone(), serde_json::Value::String(severity));
    }
  }
  let output = serde_json::json!({
      "file": file_str,
      "rules": rules_json,
      "ignorePatterns": config.ignore_patterns,
  });
  println!("{}", serde_json::to_string_pretty(&output)?);
  Ok(())
}

/// Config file names that, if present, prevent `--init` from creating a new one.
const EXISTING_CONFIG_FILES: &[&str] = &["gale.json", "gale.toml", ".stylelintrc.json"];

/// Writes a `gale.json` extending the recommended preset, refusing to
/// overwrite an existing config.
pub(crate) fn generate_init_config() -> Result<()> {
  let cwd = std::env::current_dir()?;

  for name in EXISTING_CONFIG_FILES {
    let path = cwd.join(name);
    if path.exists() {
      bail!("Configuration file already exists: {}", path.display());
    }
  }

  let config_content = r#"{
  "extends": "gale:recommended",
  "rules": {}
}
"#;

  let config_path = cwd.join("gale.json");
  std::fs::write(&config_path, config_content)?;
  println!("Created gale.json with recommended configuration.");
  Ok(())
}
