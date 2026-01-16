//! NPAP Adapter for Naia BDD tests
//!
//! This binary implements the NPAP adapter protocol for Naia:
//! - `manifest` — Emit the semantic step registry JSON
//! - `run` — Execute a resolved plan and emit a run report

use anyhow::Result;
use clap::{Parser, Subcommand};

mod manifest;
mod run;
mod bindings;
mod world;

pub use world::SmokeWorld;

/// NPAP Adapter for Naia
#[derive(Parser, Debug)]
#[command(name = "naia_namako")]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Emit the semantic step registry as JSON
    Manifest,
    /// Execute a resolved plan and emit a run report
    Run(run::RunArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Manifest => manifest::run(),
        Commands::Run(args) => run::run(args),
    }
}
