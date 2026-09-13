#![allow(
  clippy::all,
  unreachable_code,
  unused_variables,
  unused_assignments,
  dead_code
)]

pub mod data;
pub mod registry;
pub mod rule;
pub mod rules;
pub mod runner;
pub mod stylelint_version;

pub use registry::RuleRegistry;
pub use rule::{Rule, RuleContext};
pub use runner::LintRunner;
