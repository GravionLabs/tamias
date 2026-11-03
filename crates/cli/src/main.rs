//! `tamias`: the command line of tamias.

mod claude;
mod collect;
mod config;
mod today;

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgAction, Parser, Subcommand};
use tamias_core::paths;
use tamias_core::store::Store;
use tracing::Level;

/// Exit code when a provider failed (the error is recorded).
const EXIT_PROVIDER_FAILED: u8 = 1;
/// Exit code when the command could not run (config, data directory, store).
const EXIT_FATAL: u8 = 2;

/// Shows how much of your Claude and Copilot usage limits you used today
/// and whether that fits the daily budget left until the next reset.
#[derive(Debug, Parser)]
#[command(name = "tamias", version = tamias_core::version(), arg_required_else_help = true)]
struct Cli {
    /// Config file [default: config.toml in the config directory].
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Debug logging on stderr.
    #[arg(short, long, global = true, action = ArgAction::Count)]
    verbose: u8,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Poll every available provider once and store the snapshots.
    Collect,
    /// Show today's usage against today's budget.
    Today,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(cli.verbose);
    match run(&cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(EXIT_FATAL)
        }
    }
}

fn run(cli: &Cli) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let config = config::load(cli.config.as_deref())?;
    let database = paths::database_path()?;
    tracing::debug!("database {}", database.display());
    let mut store = Store::open(&database)?;
    match cli.command {
        Command::Collect => Ok(match collect::run(&config, &mut store)? {
            collect::Outcome::Ok => ExitCode::SUCCESS,
            collect::Outcome::ProviderFailed => ExitCode::from(EXIT_PROVIDER_FAILED),
        }),
        Command::Today => {
            let stdout = std::io::stdout();
            let colour = stdout.is_terminal() && !no_color();
            let table = today::run(&config, &store, colour)?;
            stdout.lock().write_all(table.as_bytes())?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// `NO_COLOR` set to a non-empty value (<https://no-color.org>).
fn no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
}

fn init_logging(verbose: u8) {
    let level = match verbose {
        0 => Level::WARN,
        1 => Level::DEBUG,
        _ => Level::TRACE,
    };
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(level)
        .with_target(false)
        .without_time()
        .with_ansi(std::io::stderr().is_terminal() && !no_color())
        .init();
}
