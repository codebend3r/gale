//! Replacing a file so that it is never seen half-written.

use std::fs::Permissions;
use std::io::Write;
use std::path::Path;

/// Replace the file at `path` with `bytes` atomically: they are written to
/// a temporary file beside it, synced, and renamed over it, so a crash or
/// a full disk leaves the old file whole rather than truncated.
///
/// The new file gets `permissions`; with `None` it is created as
/// `std::fs::write` creates a file (on unix, `0o666` less the umask, where
/// a bare temporary file would be private to its owner).
pub(crate) fn atomic_write(
  path: &Path,
  bytes: &[u8],
  permissions: Option<Permissions>,
) -> std::io::Result<()> {
  let dir = path
    .parent()
    .filter(|dir| !dir.as_os_str().is_empty())
    .unwrap_or(Path::new("."));
  let mut builder = tempfile::Builder::new();
  #[cfg(unix)]
  if permissions.is_none() {
    use std::os::unix::fs::PermissionsExt;
    builder.permissions(Permissions::from_mode(0o666));
  }
  let mut temporary = builder.tempfile_in(dir)?;
  temporary.write_all(bytes)?;
  if let Some(permissions) = permissions {
    temporary.as_file().set_permissions(permissions)?;
  }
  temporary.as_file().sync_all()?;
  temporary.persist(path).map_err(|err| err.error)?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_new_file_is_created_with_the_contents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.json");
    atomic_write(&path, b"{}", None).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
  }

  #[cfg(unix)]
  #[test]
  fn a_new_file_gets_the_permissions_a_plain_write_gives() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain");
    let atomic = dir.path().join("atomic");
    std::fs::write(&plain, "x").unwrap();
    atomic_write(&atomic, b"x", None).unwrap();
    assert_eq!(
      std::fs::metadata(&atomic).unwrap().permissions(),
      std::fs::metadata(&plain).unwrap().permissions()
    );
  }
}
