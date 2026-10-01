//! Configuration loading for the Gale CSS linter.
//!
//! Finds the config that applies to a directory or file ([`find_config`],
//! [`ConfigResolver`]), reads it in any format Stylelint accepts, JavaScript
//! included ([`load_config`]), and resolves its `extends`, built-in presets
//! and overrides into the [`GaleConfig`] a run lints with.

mod discovery;
mod extends;
mod js;
mod model;
mod plugins;
mod presets;
mod raw;
mod resolve;
mod resolver;

pub use discovery::{find_config, find_config_for_file, load_config};
pub use model::{ConfigError, FixMode, GaleConfig, ResolvedOverride, RuleConfig, Severity};
pub use plugins::{is_known_plugin, is_known_plugin_rule};
pub use presets::{recommended_rule_names, resolve_preset};
pub use raw::{ConfigFile, ConfigOverride, RuleConfigValue};
pub use resolve::{resolve_config, resolve_config_for_file};
pub use resolver::ConfigResolver;
