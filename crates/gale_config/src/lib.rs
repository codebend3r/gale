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
mod plugins;
mod presets;
mod raw;
mod resolve;
mod resolver;
mod rule_set;
mod skipped;

pub use discovery::{find_config, load_config};
pub use file_settings::{
  ConfigSource, FileSettings, Ignored, ResolverOptions, RuleSetResolver, syntax_for_custom_syntax,
  syntax_for_file,
};
pub use model::{ConfigError, FixMode, GaleConfig, ResolvedOverride, RuleConfig, Severity};
pub use plugins::is_known_plugin;
pub use presets::{recommended_rule_names, resolve_preset};
pub use raw::{ConfigFile, ConfigOverride, RuleConfigValue};
pub use rule_set::{RunSettings, WithoutConfig};
pub use skipped::{SkippedRules, unsupported_plugin_warnings};
