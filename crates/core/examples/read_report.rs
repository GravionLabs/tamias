//! How a Rust consumer (poimen) reads `tamias today --json`: run the CLI, parse with
//! [`Report::parse`] (which checks `schema_version`), look at the Claude binding window.
//!
//! `cargo run -p tamias-core --example read_report`

use tamias_core::report::Report;
use tamias_core::snapshot::{Provider, Window};

/// Share of today's Claude budget used, when there is one.
fn claude_ratio(report: &Report) -> Option<f64> {
    let provider = report
        .providers
        .iter()
        .find(|p| p.provider == Provider::Claude)?;
    let window = provider
        .windows
        .iter()
        .find(|w| w.window == Window::SevenDay)?;
    window.ratio
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // exit code 3 (a stale provider) still prints the document, so the status is not checked
    let output = tamias_core::process::command("tamias")
        .args(["today", "--json"])
        .output()?;
    let report = Report::parse(&String::from_utf8(output.stdout)?)?;
    match claude_ratio(&report) {
        Some(ratio) => println!("Claude used {:.0} % of today's budget", ratio * 100.0),
        None => println!("no Claude budget yet"),
    }
    Ok(())
}
