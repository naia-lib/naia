use anyhow::Result;
use clap::{Parser, Subcommand};

mod manifest;
mod run;
mod steps;
mod world;

#[derive(Parser, Debug)]
#[command(name = "naia_bevy_npa")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Manifest,
    Run(run::RunArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Manifest => manifest::run(),
        Commands::Run(args) => run::run(args),
    }
}
