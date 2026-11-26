//! Configuration loading for the Gale CSS linter.
//!
//! Finds the config that applies to a directory ([`find_config`]), reads it
//! in any format Stylelint accepts, JavaScript included ([`load_config`]),
//! and resolves its `extends`, built-in presets and overrides into the
//! [`GaleConfig`] a run lints with.
//!
//! [`RuleSetResolver`] is the one place a file's settings come from, for
//! the CLI and the language server alike: whether it is ignored, which
//! parser reads it, and the [`gale_linter::RuleSet`] it is linted with.

mod discovery;
mod extends;
mod file_settings;
mod js;
mod model;
mod module_resolution;
mod plugins;
mod presets;
mod raw;
mod resolve;
mod resolver;
mod root_config;
mod rule_set;
mod skipped;

pub use discovery::{find_config, load_config};
pub use file_settings::{ConfigSource, FileSettings, Ignored, ResolverOptions, RuleSetResolver};
pub use model::{
  CacheStrategy, ConfigError, FixMode, GaleConfig, ResolvedOverride, RuleConfig, Severity,
};
pub(crate) use presets::{recommended_rule_names, resolve_preset};
pub(crate) use raw::{ConfigFile, ConfigOverride, RuleConfigValue};
pub use root_config::{RootConfig, RootConfigError};
pub use rule_set::{RunSettings, WithoutConfig};
pub use skipped::{ConfigWarning, SkippedRuleWarnings, unsupported_plugin_warnings};
