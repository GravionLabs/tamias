//! Exit codes shared by `tamias` and `tamiasw`.

/// Exit code when the command could not run (invalid config, data directory, store).
pub const EXIT_ERROR: u8 = 1;
/// Exit code when at least one set-up provider is not fresh: its poll failed (`collect`) or it
/// is stale (`today`).
pub const EXIT_STALE: u8 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_the_documented_contract() {
        assert_eq!(EXIT_ERROR, 1);
        assert_eq!(EXIT_STALE, 3);
    }
}
