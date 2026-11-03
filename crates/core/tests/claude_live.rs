//! Manual run against the real endpoint with the OS credential source:
//! `cargo test -p tamias-core --test claude_live -- --ignored --nocapture`.
//! Prints only the outcome and the two utilization values, never the token.

use tamias_core::credentials::os_default;
use tamias_core::provider::claude::{ClaudeProvider, UsageClient};

#[test]
#[ignore = "hits the real Claude usage endpoint with the local credentials"]
fn live_poll_prints_utilization() {
    let source = os_default().expect("credential source");
    let provider = ClaudeProvider::new(source, UsageClient::new());
    match provider.poll(jiff::Timestamp::now()) {
        Ok(snapshots) => {
            println!("status=ok");
            for s in &snapshots {
                println!("{}={}", s.window, s.used);
            }
        }
        Err(e) => panic!("status=error {e}"),
    }
}
