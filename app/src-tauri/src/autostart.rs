//! Autostart only registers an executable that can run on its own. A dev build (`pnpm tauri dev`)
//! loads its frontend from the dev server, which does not exist after a reboot, so enabling
//! autostart there is refused.

/// Why enabling autostart is refused in a dev build.
pub const DEV_BUILD: &str = "Autostart is only available in an installed build";

/// Whether this build can run on its own: not a debug (dev) build.
#[must_use]
pub const fn available() -> bool {
    !cfg!(debug_assertions)
}

/// Whether enabling autostart is allowed in a build that is `dev` (a debug build) or not.
///
/// # Errors
///
/// [`DEV_BUILD`] in a dev build.
pub fn check_enable(dev: bool) -> Result<(), &'static str> {
    if dev { Err(DEV_BUILD) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_enabling_in_a_dev_build() {
        assert_eq!(
            check_enable(true),
            Err("Autostart is only available in an installed build")
        );
    }

    #[test]
    fn allows_enabling_in_an_installed_build() {
        assert_eq!(check_enable(false), Ok(()));
    }

    #[test]
    fn availability_follows_the_build_profile() {
        assert_eq!(available(), !cfg!(debug_assertions));
    }
}
