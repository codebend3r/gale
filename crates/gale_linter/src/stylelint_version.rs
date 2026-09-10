use std::sync::OnceLock;

static STYLELINT_MAJOR: OnceLock<u32> = OnceLock::new();

/// Returns the major version of the locally-installed Stylelint package by
/// reading `node_modules/stylelint/package.json` from the current working
/// directory (walking up until found).  Returns 16 if not found, matching the
/// behaviour of modern Stylelint.
pub fn stylelint_major_version() -> u32 {
  *STYLELINT_MAJOR.get_or_init(detect)
}

fn detect() -> u32 {
  let cwd = match std::env::current_dir() {
    Ok(d) => d,
    Err(_) => return 16,
  };

  let mut dir = cwd.as_path();
  loop {
    let pkg = dir.join("node_modules/stylelint/package.json");
    if pkg.exists() {
      if let Ok(content) = std::fs::read_to_string(&pkg) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
          if let Some(ver) = json.get("version").and_then(|v| v.as_str()) {
            if let Some(major_str) = ver.split('.').next() {
              if let Ok(major) = major_str.parse::<u32>() {
                return major;
              }
            }
          }
        }
      }
    }
    match dir.parent() {
      Some(p) => dir = p,
      None => break,
    }
  }
  16
}
