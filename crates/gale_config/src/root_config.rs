//! The config a run starts from, found the same way by the CLI and the
//! language server: the explicitly named file, else the closest config.

use std::path::{Path, PathBuf};

use crate::discovery::{find_config, load_config};
use crate::file_settings::ConfigSource;
use crate::model::{ConfigError, GaleConfig};

/// The root config: the `--config` file, else the closest config file to
/// the working directory (or the editor's workspace root).
#[derive(Debug, Clone)]
pub struct RootConfig {
  pub config: GaleConfig,
  /// Where the closest config was found, which files without a closer
  /// config fall back on; `None` for an explicit config, or with no config
  /// file.
  pub path: Option<PathBuf>,
  /// Whether `config` was named explicitly, so it applies to every file.
  explicit: bool,
}

/// A root config file that exists but could not be loaded.
#[derive(Debug)]
pub struct RootConfigError {
  pub path: PathBuf,
  pub error: ConfigError,
  explicit: bool,
}

impl RootConfigError {
  /// The root config as if the file that failed to load were empty, for a
  /// frontend (the language server) that reports the failure and carries
  /// on rather than stopping.
  pub fn into_empty(self) -> RootConfig {
    RootConfig {
      config: GaleConfig::default(),
      path: (!self.explicit).then_some(self.path),
      explicit: self.explicit,
    }
  }
}

impl RootConfig {
  /// Load `explicit` when given, else the closest config to `dir`; with
  /// neither, an empty config and no path.
  pub fn load(explicit: Option<&Path>, dir: &Path) -> Result<Self, RootConfigError> {
    let (path, is_explicit) = match explicit {
      Some(path) => (path.to_path_buf(), true),
      None => match find_config(dir) {
        Some(path) => (path, false),
        None => {
          return Ok(Self {
            config: GaleConfig::default(),
            path: None,
            explicit: false,
          });
        }
      },
    };
    match load_config(&path) {
      Ok(config) => Ok(Self {
        config,
        path: (!is_explicit).then_some(path),
        explicit: is_explicit,
      }),
      Err(error) => Err(RootConfigError {
        path,
        error,
        explicit: is_explicit,
      }),
    }
  }

  /// Where a resolver built on this root gets each file's config: this one
  /// for every file when it was named explicitly, else each file's closest
  /// config, falling back on this one.
  pub fn into_source(self) -> ConfigSource {
    if self.explicit {
      return ConfigSource::Explicit(self.config);
    }
    ConfigSource::Closest {
      root: self.path.map(|path| (path, self.config)),
    }
  }
}
