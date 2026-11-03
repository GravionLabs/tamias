//! `tamiasw`: `tamias collect` without a console window, for the Windows Task Scheduler.
//!
//! A console-subsystem exe flashes a window on every run; this one is a GUI-subsystem exe that
//! runs exactly `collect` through the same code. It is built on every OS but only used on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]
// The shared modules also serve `tamias today`, which this binary does not include.
#![allow(dead_code)]

#[path = "../claude.rs"]
mod claude;
#[path = "../collect.rs"]
mod collect;
#[path = "../collect_log.rs"]
mod collect_log;
#[path = "../config.rs"]
mod config;

use std::process::ExitCode;

use tamias_core::paths;
use tamias_core::store::Store;

/// Same exit codes as `tamias collect`: 0 ok, 1 could not run, 3 a poll failed.
fn main() -> ExitCode {
    match run() {
        Ok(collect::Outcome::Ok) => ExitCode::SUCCESS,
        Ok(collect::Outcome::ProviderFailed) => ExitCode::from(3),
        Err(e) => {
            // Without a console this goes nowhere; kept for `tamiasw` run from a terminal.
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<collect::Outcome, Box<dyn std::error::Error>> {
    let config = config::load(None)?;
    let data_dir = paths::data_dir()?;
    let log = collect_log::CollectLog::in_data_dir(&data_dir);
    let mut store = Store::open(data_dir.join(paths::DATABASE_FILE))?;
    Ok(collect::run(&config, &mut store, &log)?)
}
