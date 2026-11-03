//! Core of tamias: usage providers, the snapshot store and the daily budget calculation.

pub mod paths;
pub mod snapshot;

/// Version of `tamias-core`, the workspace version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::version;

    #[test]
    fn version_is_a_semver_triple() {
        let parts: Vec<&str> = version().split('.').collect();
        assert_eq!(parts.len(), 3, "version {} is not x.y.z", version());
        assert!(parts.iter().all(|p| p.parse::<u64>().is_ok()));
    }
}
