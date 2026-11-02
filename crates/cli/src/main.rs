//! `tamias`: the command line of tamias.

use clap::Parser;

/// Shows how much of your Claude and Copilot usage limits you used today
/// and whether that fits the daily budget left until the next reset.
#[derive(Debug, Parser)]
#[command(name = "tamias", version = tamias_core::version(), arg_required_else_help = true)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}
