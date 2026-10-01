#![allow(
  clippy::all,
  unreachable_code,
  unused_variables,
  unused_assignments,
  dead_code
)]

pub mod autoprefixable;
pub mod css_tokenizer;
pub mod custom_message;
pub mod data;
pub mod embedded;
pub mod empty_lines;
pub mod known_rules;
pub mod panic_guard;
pub mod pattern;
pub mod postcss_tree;
pub mod registry;
pub mod rule;
pub mod rules;
pub mod runner;
pub mod selector;
pub mod source_text;
pub mod standard_syntax;
pub mod style_rules;
pub mod stylelint_version;
pub mod value_parser;

pub use registry::RuleRegistry;
pub use rule::{Rule, RuleContext};
pub use runner::LintRunner;
