use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use genhoi::adapter::MockScenario;
use genhoi::config::AppConfig;
use genhoi::memory::MemoryStore;
use genhoi::simulation::{analyze_state, format_human, run};
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
        /// Record the state and decision in the configured SQLite database.
        #[arg(long)]
        record: bool,
    },
    /// Replay and re-evaluate every normalized state in a recorded session.
    Replay {
        session: String,
        #[arg(long)]
        json: bool,
    },
    /// Show resolved local paths and safety settings.
    Config,
    /// Show SQLite schema, counters and file size.
    DbInfo,
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
        Command::Simulate {
            scenario,
            json,
            record,
        } => {
            let report = run(&config, scenario)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", format_human(&report));
            }
            if record {
                let mut store = MemoryStore::open(&config.database_path())?;
                let session = store.begin_session("simulation")?;
                store.record_report(&session, &report)?;
                store.finish_session(&session)?;
                eprintln!("Recorded session: {session}");
            }
        }
        Command::Replay { session, json } => {
            let store = MemoryStore::open(&config.database_path())?;
            let states = store.load_session(&session)?;
            let reports = states
                .into_iter()
                .map(|state| {
                    let scenario = state.strategic_summary.clone();
                    analyze_state(&config, state, scenario)
                })
                .collect::<Result<Vec<_>>>()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&reports)?);
            } else {
                for report in reports {
                    print!("{}", format_human(&report));
                }
            }
        }
        Command::Config => {
            println!("observer_only = {}", config.agent.observer_only);
            println!("executor_enabled = {}", config.agent.executor_enabled);
            println!("llm_enabled = {}", config.llm.enabled);
            println!("data_dir = {}", config.data_dir().display());
            println!("database = {}", config.database_path().display());
        }
        Command::DbInfo => {
            let store = MemoryStore::open(&config.database_path())?;
            let info = store.info()?;
            println!("schema_version = {}", info.schema_version);
            println!("sessions = {}", info.sessions);
            println!("states = {}", info.states);
            println!("decisions = {}", info.decisions);
            println!("episodes = {}", info.episodes);
            println!("lessons = {}", info.lessons);
            println!("q_values = {}", info.q_values);
            println!("size_bytes = {}", info.size_bytes);
        }
    }
    Ok(())
}
