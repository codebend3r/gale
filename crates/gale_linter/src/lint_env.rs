//! What a lint run knows about the project beyond its config.

use std::path::Path;

use crate::stylelint_version;

/// Facts about the project that change what some rules report, found once
/// per run and handed to every rule through its [`RuleContext`].
///
/// They are not rule options: a user never writes them, and keeping them out
/// of the options means a rule's options stay exactly what the config said.
///
/// [`RuleContext`]: crate::RuleContext
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct LintEnv {
  /// The `(major, minor, patch)` version of the Stylelint installed for the
  /// project, when there is one.  Gale follows the current Stylelint, but a
  /// few rules mirror what an older one reports so a project that pins it
  /// sees the same results.
  pub stylelint_version: Option<(u32, u32, u32)>,
}

impl LintEnv {
  /// The environment of a project rooted at `dir`: the Stylelint installed
  /// in `dir` or the closest ancestor that has one.
  pub fn detect(dir: &Path) -> Self {
    Self {
      stylelint_version: stylelint_version::installed_version_from(dir),
    }
  }

  /// Whether the project's Stylelint is older than `major`.  Without an
  /// installed Stylelint the current one is assumed, which is not.
  pub fn stylelint_older_than(&self, major: u32) -> bool {
    self
      .stylelint_version
      .is_some_and(|(installed, _, _)| installed < major)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn detects_the_closest_installed_stylelint() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = tmp.path().join("node_modules/stylelint");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(pkg.join("package.json"), r#"{"version": "14.16.1"}"#).unwrap();
    let nested = tmp.path().join("packages/app");
    std::fs::create_dir_all(&nested).unwrap();

    let env = LintEnv::detect(&nested);
    assert_eq!(env.stylelint_version, Some((14, 16, 1)));
    assert!(env.stylelint_older_than(15));
    assert!(!env.stylelint_older_than(14));
    assert!(!LintEnv::default().stylelint_older_than(15));
  }
}
