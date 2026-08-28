use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use genhoi::adapter::MockScenario;
use genhoi::config::AppConfig;
use genhoi::simulation::{format_human, run};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "genhoi", version, about)]
struct Cli {
    /// Use a complete TOML configuration instead of the embedded defaults.
    #[arg(long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Analyze one deterministic synthetic HOI4 scenario.
    Simulate {
        #[arg(long, value_enum, default_value_t = MockScenario::DeepSalient)]
        scenario: MockScenario,
        /// Emit the complete normalized state and decision as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show resolved local paths and safety settings.
    Config,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_target(false)
        .compact()
        .init();

    let cli = Cli::parse();
    let config = AppConfig::load(cli.config.as_deref())?;
    match cli.command {
        Command::Simulate { scenario, json } => {
            let report = run(&config, scenario)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", format_human(&report));
            }
        }
        Command::Config => {
            println!("observer_only = {}", config.agent.observer_only);
            println!("executor_enabled = {}", config.agent.executor_enabled);
            println!("llm_enabled = {}", config.llm.enabled);
            println!("data_dir = {}", config.data_dir().display());
            println!("database = {}", config.database_path().display());
        }
    }
    Ok(())
}
