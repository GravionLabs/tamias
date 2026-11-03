//! Credentials file source (Linux, Windows).

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use super::{CredentialSource, Secret, parse_credentials};
use crate::provider::claude::{Error, Result};

/// Reads `.credentials.json` below the Claude config directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSource {
    path: PathBuf,
}

impl FileSource {
    /// File name of the credentials below the config directory.
    pub const FILE_NAME: &'static str = ".credentials.json";

    /// Source reading the credentials file at `path`.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Source for the current environment: `$CLAUDE_CONFIG_DIR/.credentials.json`, else
    /// `<home>/.claude/.credentials.json` (`%USERPROFILE%` on Windows).
    ///
    /// # Errors
    ///
    /// [`Error::NoCredentials`] when neither `CLAUDE_CONFIG_DIR` nor a home directory is set.
    pub fn from_env() -> Result<Self> {
        Self::resolve(std::env::var_os("CLAUDE_CONFIG_DIR"), std::env::home_dir())
            .map(Self::new)
            .ok_or_else(|| {
                Error::NoCredentials(
                    "cannot locate the Claude credentials: neither CLAUDE_CONFIG_DIR nor a home directory is set"
                        .to_owned(),
                )
            })
    }

    /// Path of the credentials file for the given `CLAUDE_CONFIG_DIR` value and home directory.
    /// An empty `CLAUDE_CONFIG_DIR` counts as unset.
    #[must_use]
    pub fn resolve(config_dir: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
        let dir = match config_dir.filter(|d| !d.is_empty()) {
            Some(dir) => PathBuf::from(dir),
            None => home?.join(".claude"),
        };
        Some(dir.join(Self::FILE_NAME))
    }

    /// Path this source reads.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl CredentialSource for FileSource {
    fn claude_token(&self) -> Result<Secret> {
        let bytes = std::fs::read(&self.path).map_err(|e| {
            Error::NoCredentials(if e.kind() == io::ErrorKind::NotFound {
                format!("credentials file not found: {}", self.path.display())
            } else {
                format!("cannot read {}: {e}", self.path.display())
            })
        })?;
        parse_credentials(&bytes)
    }
}
