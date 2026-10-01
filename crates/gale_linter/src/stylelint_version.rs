use std::sync::OnceLock;

static STYLELINT_VERSION: OnceLock<Option<(u32, u32)>> = OnceLock::new();

/// Returns the major version of the locally-installed Stylelint package by
/// reading `node_modules/stylelint/package.json` from the current working
/// directory (walking up until found).  Returns 16 if not found, matching the
/// behaviour of modern Stylelint.
pub fn stylelint_major_version() -> u32 {
  installed_version().map_or(16, |(major, _)| major)
}

/// Whether the installed Stylelint appends ` (rule-name)` to a rule's
/// custom `message`, as every Stylelint since 16.25 does.  Older versions
/// print a custom message as written.  Without an installed Stylelint the
/// current behaviour is assumed.
pub fn appends_rule_name_to_custom_messages() -> bool {
  installed_version().is_none_or(|version| version >= (16, 25))
}

/// The `(major, minor)` version of the installed Stylelint, found once per
/// process, or `None` when there is none.
fn installed_version() -> Option<(u32, u32)> {
  *STYLELINT_VERSION.get_or_init(detect)
}

/// Walks up from the current directory looking for an installed Stylelint
/// and parses its version.
fn detect() -> Option<(u32, u32)> {
  let cwd = std::env::current_dir().ok()?;
  let mut dir = cwd.as_path();
  loop {
    let pkg = dir.join("node_modules/stylelint/package.json");
    if let Some(version) = std::fs::read_to_string(&pkg)
      .ok()
      .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
      .and_then(|json| json.get("version")?.as_str().and_then(parse_version))
    {
      return Some(version);
    }
    dir = dir.parent()?;
  }
}

/// `(major, minor)` from a version string such as `16.26.1`; a missing
/// minor counts as 0.
fn parse_version(version: &str) -> Option<(u32, u32)> {
  let mut parts = version.split('.');
  let major = parts.next()?.parse().ok()?;
  let minor = parts
    .next()
    .and_then(|minor| minor.parse().ok())
    .unwrap_or(0);
  Some((major, minor))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_major_and_minor() {
    assert_eq!(parse_version("16.26.1"), Some((16, 26)));
    assert_eq!(parse_version("17.0.0-beta.1"), Some((17, 0)));
    assert_eq!(parse_version("13"), Some((13, 0)));
    assert_eq!(parse_version("x.1"), None);
  }
}
