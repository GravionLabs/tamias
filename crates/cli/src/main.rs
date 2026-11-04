//! `tamias`: the command line of tamias.

mod claude;
mod collect;
mod collect_log;
mod copilot;
mod exit;
mod service;
mod short;
mod today;
mod unavailable;

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use tamias_core::paths;
use tamias_core::store::Store;
use tracing::Level;

use crate::exit::{EXIT_ERROR, EXIT_STALE};

/// Exit codes shared by `collect` and `today`, shown in their `--help`.
const EXIT_CODES: &str = "\
Exit codes:
  0  every set-up provider is ok (unavailable providers do not count)
  1  error: tamias could not run (invalid config, unreadable store); collect writes it to collect.log only, no provider turns stale
  2  invalid command line
  3  at least one set-up provider is not fresh (collect: its poll failed, recorded in the store and collect.log; today: it is stale)";

/// Shows how much of your Claude usage limits you used today
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
    #[command(after_help = EXIT_CODES)]
    Collect,
    /// Show today's usage against today's budget.
    #[command(after_help = EXIT_CODES)]
    Today(TodayArgs),
    /// Run the collector in the background (install, uninstall, status).
    #[command(subcommand)]
    Service(service::Action),
}

#[derive(Debug, Args)]
struct TodayArgs {
    /// One compact line for status lines: `C 62% · CP 41%` (binding windows, `?` when stale).
    #[arg(long, conflicts_with = "json")]
    short: bool,
    /// Print one JSON document (schema docs/schema/today-v1.schema.json) instead of the table;
    /// never coloured.
    #[arg(long)]
    json: bool,
    /// When to colour the output; `auto` colours only a terminal without `NO_COLOR`
    /// (ignored with `--json`).
    #[arg(long, value_enum, value_name = "WHEN", default_value_t = ColorChoice::Auto)]
    color: ColorChoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ColorChoice {
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    /// Whether to emit ANSI colours on a stream that is (`terminal`) a TTY.
    fn enabled(self, terminal: bool, no_color: bool) -> bool {
        match self {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => terminal && !no_color,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(cli.verbose);
    match run(&cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(EXIT_ERROR)
        }
    }
}

fn run(cli: &Cli) -> Result<ExitCode, Box<dyn std::error::Error>> {
    if matches!(cli.command, Command::Collect) {
        return Ok(match collect::run_logged(cli.config.as_deref())? {
            collect::Outcome::Ok => ExitCode::SUCCESS,
            collect::Outcome::ProviderFailed => ExitCode::from(EXIT_STALE),
        });
    }
    let config = tamias_core::config::load(cli.config.as_deref())?;
    let data_dir = paths::data_dir()?;
    let database = data_dir.join(paths::DATABASE_FILE);
    tracing::debug!("database {}", database.display());
    let log = collect_log::CollectLog::in_data_dir(&data_dir);
    match cli.command {
        Command::Collect => unreachable!("handled before the config is loaded"),
        Command::Today(ref args) => {
            // Whole seconds: `generated_at` of the JSON is RFC 3339 without a fraction.
            let now = Timestamp::from_second(Timestamp::now().as_second())?;
            let tz = TimeZone::system();
            let store = Store::open(&database)?;
            let today = today::read(&config, &store, now, &tz)?;
            let stdout = std::io::stdout();
            let colour = args.color.enabled(stdout.is_terminal(), no_color());
            let out = if args.json {
                today::json(&today.report)?
            } else if args.short {
                short::render(&today.report, colour) + "\n"
            } else {
                today::render(&today.lines, now, &tz, colour)
            };
            stdout.lock().write_all(out.as_bytes())?;
            Ok(if today.any_stale() {
                ExitCode::from(EXIT_STALE)
            } else {
                ExitCode::SUCCESS
            })
        }
        Command::Service(action) => {
            let scheduler = service::platform()?;
            let open_store = || Ok(Store::open(&database)?);
            let text = service::execute(action, scheduler.as_ref(), open_store, &log.path())?;
            std::io::stdout().lock().write_all(text.as_bytes())?;
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

#[cfg(test)]
mod tests {
    use super::ColorChoice;

    #[test]
    fn auto_colours_only_a_terminal_without_no_color() {
        assert!(ColorChoice::Auto.enabled(true, false));
        assert!(!ColorChoice::Auto.enabled(true, true));
        assert!(!ColorChoice::Auto.enabled(false, false));
    }

    #[test]
    fn always_and_never_ignore_the_terminal_and_no_color() {
        for (terminal, no_color) in [(true, true), (false, false), (false, true)] {
            assert!(ColorChoice::Always.enabled(terminal, no_color));
            assert!(!ColorChoice::Never.enabled(terminal, no_color));
        }
    }
}
