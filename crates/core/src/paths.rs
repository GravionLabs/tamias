//! Per-user data and config directories of tamias.
//!
//! | OS      | data and config                         |
//! |---------|-----------------------------------------|
//! | macOS   | `~/Library/Application Support/tamias`  |
//! | Linux   | `$XDG_DATA_HOME/tamias`, `$XDG_CONFIG_HOME/tamias` |
//! | Windows | `%APPDATA%\tamias`, `%APPDATA%\tamias\config` |
//!
//! `TAMIAS_DATA_DIR` overrides the data directory (used by tests and portable setups).

use std::ffi::OsString;
use std::path::PathBuf;

use directories::ProjectDirs;

/// Environment variable that overrides the data directory.
pub const DATA_DIR_ENV: &str = "TAMIAS_DATA_DIR";

/// File name of the snapshot database inside the data directory.
pub const DATABASE_FILE: &str = "tamias.db";

/// No home directory could be determined for the current user.
#[derive(Debug, thiserror::Error)]
#[error("no home directory found for the current user; set {DATA_DIR_ENV}")]
pub struct NoHomeDir;

/// Data directory: `TAMIAS_DATA_DIR` when set and non-empty, else the OS data directory.
///
/// # Errors
///
/// [`NoHomeDir`] when neither the override nor a home directory is available.
pub fn data_dir() -> Result<PathBuf, NoHomeDir> {
    resolve_data_dir(std::env::var_os(DATA_DIR_ENV), project_dirs().as_ref())
}

/// Config directory of the OS.
///
/// # Errors
///
/// [`NoHomeDir`] when no home directory is available.
pub fn config_dir() -> Result<PathBuf, NoHomeDir> {
    project_dirs()
        .map(|dirs| dirs.config_dir().to_path_buf())
        .ok_or(NoHomeDir)
}

/// Path of the snapshot database: [`DATABASE_FILE`] inside [`data_dir`].
///
/// # Errors
///
/// [`NoHomeDir`] as for [`data_dir`].
pub fn database_path() -> Result<PathBuf, NoHomeDir> {
    data_dir().map(|dir| dir.join(DATABASE_FILE))
}

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "tamias")
}

fn resolve_data_dir(
    env_override: Option<OsString>,
    dirs: Option<&ProjectDirs>,
) -> Result<PathBuf, NoHomeDir> {
    if let Some(dir) = env_override.filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    dirs.map(os_data_dir).ok_or(NoHomeDir)
}

/// `directories` puts Windows data under `%APPDATA%\tamias\data`; tamias uses `%APPDATA%\tamias`.
fn os_data_dir(dirs: &ProjectDirs) -> PathBuf {
    let data = dirs.data_dir();
    if cfg!(windows) {
        data.parent().unwrap_or(data).to_path_buf()
    } else {
        data.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use super::{project_dirs, resolve_data_dir};

    #[test]
    fn override_wins_over_the_os_directory() {
        let dir = resolve_data_dir(
            Some(OsString::from("/tmp/tamias-test")),
            project_dirs().as_ref(),
        );
        assert_eq!(dir.unwrap(), PathBuf::from("/tmp/tamias-test"));
    }

    #[test]
    fn override_is_used_without_a_home_directory() {
        let dir = resolve_data_dir(Some(OsString::from("data")), None);
        assert_eq!(dir.unwrap(), PathBuf::from("data"));
    }

    #[test]
    fn empty_override_falls_back_to_the_os_directory() {
        let Some(dirs) = project_dirs() else { return };
        let dir = resolve_data_dir(Some(OsString::new()), Some(&dirs)).unwrap();
        assert_eq!(dir.file_name(), Some(Path::new("tamias").as_os_str()));
    }

    #[test]
    fn os_directory_ends_in_tamias() {
        let Some(dirs) = project_dirs() else { return };
        let dir = resolve_data_dir(None, Some(&dirs)).unwrap();
        assert_eq!(dir.file_name(), Some(Path::new("tamias").as_os_str()));
        assert!(dir.is_absolute());
    }

    #[test]
    fn no_override_and_no_home_is_an_error() {
        let err = resolve_data_dir(None, None).unwrap_err();
        assert!(err.to_string().contains("TAMIAS_DATA_DIR"));
    }
}
