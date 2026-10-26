//! One configuration-comment policy for standalone sheets and host documents.

use std::collections::HashSet;

use gale_diagnostics::{Diagnostic, Severity};

use super::{DisableReports, DisableSettings, DisabledRanges, apply};
use crate::registry::resolve_deprecated_alias;
use crate::runner::DisableOptions;

#[derive(Debug, Clone, Default)]
pub struct DisablePolicy {
  pub options: DisableOptions,
  pub default_severity: Option<Severity>,
  pub configured: HashSet<String>,
  pub forbids_disable: HashSet<String>,
}

impl DisablePolicy {
  pub(crate) fn apply(
    &self,
    diagnostics: &mut Vec<Diagnostic>,
    ranges: &DisabledRanges,
    line_of: &dyn Fn(usize) -> usize,
    file_path: &str,
  ) {
    let settings = DisableSettings {
      suppress: !self.options.ignore,
      reports: DisableReports {
        needless: self.options.report_needless,
        invalid_scope: self.options.report_invalid_scope,
        descriptionless: self.options.report_descriptionless,
        unscoped: self.options.report_unscoped,
        severity: self.default_severity.unwrap_or(Severity::Error),
      },
      is_configured: &|name| self.configured.contains(canonical(name)),
      forbids_disable: &|name| {
        self.forbids_disable.contains(name) || self.forbids_disable.contains(canonical(name))
      },
      file_path,
    };
    apply(diagnostics, ranges, line_of, &settings);
  }
}

fn canonical(name: &str) -> &str {
  resolve_deprecated_alias(name).unwrap_or(name)
}
