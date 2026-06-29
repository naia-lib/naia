//! Spec tool for Naia: golden-trace record/check and scenario verification.
//!
//! Subcommands:
//! - `traces record <key>` — run the named scenario with trace capture enabled
//!   and write the resulting trace to `test/golden_traces/<key>.json`.
//! - `traces check` — re-run all scenarios that have a golden trace file and
//!   assert the new trace matches.

use anyhow::Result;
use clap::{Parser, Subcommand};

mod traces;

/// Naia spec tool
#[derive(Parser, Debug)]
#[command(name = "naia_spec_tool")]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Golden wire-trace commands
    #[command(subcommand)]
    Traces(traces::TracesCommand),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Traces(cmd) => traces::run(cmd),
    }
}
