use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::{Parser, Subcommand};
use genhoi::adapter::{AdapterError, GameAdapter, MockScenario};
use genhoi::config::AppConfig;
use genhoi::doctrine::{DoctrineEngine, LessonStatus};
use genhoi::learning::{QLearner, abstract_state};
use genhoi::memory::MemoryStore;
use genhoi::metrics::FrontMetrics;
use genhoi::performance::PerformanceMonitor;
use genhoi::planner::GameActionKind;
use genhoi::reasoner::{ConfiguredReasoner, MemoryAwareReasoner};
use genhoi::runtime::AgentRuntime;
use genhoi::simulation::{analyze_state, format_human, run};
use genhoi::telemetry::TelemetryGameAdapter;
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
    /// List lessons or derive proposals from one recorded session.
    Lessons {
        #[arg(long, value_name = "SESSION")]
        generate_from: Option<String>,
    },
    /// List active doctrine entries or explicitly review a proposed lesson.
    Doctrine {
        #[arg(long, conflicts_with_all = ["reject", "obsolete"])]
        activate: Option<i64>,
        #[arg(long, conflicts_with_all = ["activate", "obsolete"])]
        reject: Option<i64>,
        #[arg(long, conflicts_with_all = ["activate", "reject"])]
        obsolete: Option<i64>,
    },
    /// Generate and store an after-action review for a session.
    Review { session: String },
    /// Read and analyze one snapshot from a versioned HOI4 telemetry log.
    Observe {
        #[arg(long, value_name = "FILE")]
        telemetry: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Follow a telemetry log in observer-only mode.
    Run {
        #[arg(long, value_name = "FILE")]
        telemetry: PathBuf,
        /// Stop after N observations; zero means follow until interrupted.
        #[arg(long, default_value_t = 0)]
        max_observations: usize,
    },
    /// Show current process and database resource statistics.
    Stats,
    /// Benchmark deterministic analysis over all mock scenarios.
    Benchmark {
        #[arg(long, default_value_t = 10_000)]
        iterations: usize,
    },
    /// Attach a verified outcome to the last state of a recorded session.
    RecordOutcome {
        session: String,
        #[arg(long, value_enum)]
        action: GameActionKind,
        #[arg(long)]
        reward: f32,
        #[arg(long, default_value = "{}")]
        outcome_json: String,
    },
}

#[allow(clippy::too_many_lines)] // The exhaustive CLI dispatcher keeps command effects auditable.
fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = AppConfig::load(cli.config.as_deref())?;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(&config.logging.level)),
        )
        .with_target(false)
        .compact()
        .init();
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
            println!("logging_level = {}", config.logging.level);
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
        Command::Lessons { generate_from } => {
            let store = MemoryStore::open(&config.database_path())?;
            if let Some(session) = generate_from {
                let episodes = store.session_episodes(&session)?;
                for lesson in DoctrineEngine.derive(&episodes) {
                    let id = store.save_lesson(&lesson)?;
                    println!("proposed lesson #{id}: {}", lesson.proposed_doctrine);
                }
            } else {
                for lesson in store.lessons(None)? {
                    println!(
                        "#{} [{}] confidence={:.2}: {}",
                        lesson.id,
                        lesson.draft.status,
                        lesson.draft.confidence,
                        lesson.draft.proposed_doctrine
                    );
                }
            }
        }
        Command::Doctrine {
            activate,
            reject,
            obsolete,
        } => {
            let mut store = MemoryStore::open(&config.database_path())?;
            if let Some(id) = activate {
                store.set_lesson_status(id, LessonStatus::Active)?;
            } else if let Some(id) = reject {
                store.set_lesson_status(id, LessonStatus::Rejected)?;
            } else if let Some(id) = obsolete {
                store.set_lesson_status(id, LessonStatus::Obsolete)?;
            }
            for lesson in store.lessons(Some(LessonStatus::Active))? {
                println!(
                    "#{} confidence={:.2}: {}",
                    lesson.id, lesson.draft.confidence, lesson.draft.proposed_doctrine
                );
            }
        }
        Command::Review { session } => {
            let store = MemoryStore::open(&config.database_path())?;
            let episodes = store.session_episodes(&session)?;
            let review = DoctrineEngine.after_action_review(&session, &episodes);
            store.save_review(&review)?;
            println!("{}", review.report);
        }
        Command::Observe { telemetry, json } => {
            let mut adapter = TelemetryGameAdapter::new(telemetry);
            let state = adapter.observe()?;
            let scenario = state.strategic_summary.clone();
            let report = analyze_state(&config, state, scenario)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", format_human(&report));
            }
        }
        Command::Run {
            telemetry,
            max_observations,
        } => run_observer(&config, telemetry, max_observations)?,
        Command::Stats => {
            let store = MemoryStore::open(&config.database_path())?;
            let info = store.info()?;
            let mut monitor = PerformanceMonitor::new()?;
            let snapshot = monitor.snapshot(info.size_bytes, &config.performance);
            println!("ram_mb = {:.1}", snapshot.ram_mb);
            println!("cpu_percent = {:.2}", snapshot.cpu_percent);
            println!("sqlite_size_bytes = {}", snapshot.sqlite_size_bytes);
            println!(
                "decision_latency_ms_average = {:.2}",
                snapshot.decision_latency_ms_average
            );
            for warning in snapshot.warnings {
                println!("warning = {warning}");
            }
        }
        Command::Benchmark { iterations } => benchmark(&config, iterations)?,
        Command::RecordOutcome {
            session,
            action,
            reward,
            outcome_json,
        } => record_outcome(&config, &session, action, reward, &outcome_json)?,
    }
    Ok(())
}

fn run_observer(config: &AppConfig, telemetry: PathBuf, maximum: usize) -> Result<()> {
    anyhow::ensure!(
        config.agent.observer_only,
        "run currently requires observer-only mode"
    );
    let adapter = TelemetryGameAdapter::new(telemetry);
    let reasoner = MemoryAwareReasoner::new(
        ConfiguredReasoner::new(&config.llm),
        MemoryStore::open(&config.database_path())?,
        config.agent.learning_enabled,
    );
    let mut runtime = AgentRuntime::new(adapter, reasoner, config.clone());
    let mut store = MemoryStore::open(&config.database_path())?;
    let mut monitor = PerformanceMonitor::new()?;
    let session = store.begin_session("telemetry-observer")?;
    let started = Instant::now();
    let mut observed = 0_usize;
    loop {
        let decision_started = Instant::now();
        match runtime.tick(started.elapsed()) {
            Ok(cycle) => {
                monitor.record_decision(decision_started.elapsed());
                if let Some(duration) = runtime
                    .reasoner_mut()
                    .inner_mut()
                    .take_last_inference_duration()
                {
                    monitor.record_llm_inference(duration);
                }
                for event in &cycle.events {
                    tracing::info!(?event, "GENERAL STAFF event");
                }
                if let Some(decision) = &cycle.strategic_decision {
                    tracing::info!(
                        intent = ?decision.intent,
                        priority = decision.priority,
                        reason = %decision.reason,
                        "DECISION"
                    );
                }
                if let Some(reward) = &cycle.reward {
                    tracing::info!(reward = reward.total, "LEARNING outcome reward");
                }
                let scenario = cycle.state.strategic_summary.clone();
                let report = analyze_state(config, cycle.state, scenario)?;
                print!("{}", format_human(&report));
                store.record_report(&session, &report)?;
                observed = observed.saturating_add(1);
                if maximum > 0 && observed >= maximum {
                    break;
                }
            }
            Err(AdapterError::Exhausted) => {
                thread::sleep(Duration::from_secs(config.agent.observer_interval_seconds));
            }
            Err(error) => return Err(error.into()),
        }
    }
    store.finish_session(&session)?;
    let episodes = store.session_episodes(&session)?;
    let review = DoctrineEngine.after_action_review(&session, &episodes);
    store.save_review(&review)?;
    for lesson in DoctrineEngine.derive(&episodes) {
        store.save_lesson(&lesson)?;
    }
    let info = store.info()?;
    let snapshot = monitor.snapshot(info.size_bytes, &config.performance);
    tracing::info!(
        ram_mb = snapshot.ram_mb,
        cpu_percent = snapshot.cpu_percent,
        average_decision_ms = snapshot.decision_latency_ms_average,
        "PERFORMANCE"
    );
    eprintln!("Recorded session: {session}");
    Ok(())
}

fn benchmark(config: &AppConfig, iterations: usize) -> Result<()> {
    anyhow::ensure!(iterations > 0, "iterations must be greater than zero");
    let scenarios = [
        MockScenario::StableFront,
        MockScenario::LowSupply,
        MockScenario::Breakthrough,
        MockScenario::DeepSalient,
        MockScenario::EncirclementRisk,
        MockScenario::EnemyCollapse,
    ];
    let started = Instant::now();
    for index in 0..iterations {
        let scenario = scenarios[index % scenarios.len()];
        std::hint::black_box(run(config, scenario)?);
    }
    let elapsed = started.elapsed();
    let average_micros = elapsed.as_secs_f64() * 1_000_000.0 / usize_as_f64(iterations);
    println!("iterations = {iterations}");
    println!("total_ms = {:.2}", elapsed.as_secs_f64() * 1000.0);
    println!("average_us = {average_micros:.2}");
    Ok(())
}

fn record_outcome(
    config: &AppConfig,
    session: &str,
    action: GameActionKind,
    reward: f32,
    outcome_json: &str,
) -> Result<()> {
    anyhow::ensure!(config.agent.learning_enabled, "learning is disabled");
    anyhow::ensure!(reward.is_finite(), "reward must be finite");
    let outcome: serde_json::Value = serde_json::from_str(outcome_json)?;
    let store = MemoryStore::open(&config.database_path())?;
    let states = store.load_session(session)?;
    let state = states
        .last()
        .ok_or_else(|| anyhow::anyhow!("session has no states"))?;
    let front = state
        .fronts
        .first()
        .ok_or_else(|| anyhow::anyhow!("last state has no fronts"))?;
    let metrics = FrontMetrics::calculate(front, config.risk.minimum_neck_width_km);
    let features = metrics.feature_vector();
    let state_key = abstract_state(&metrics);
    let action = action.to_string();
    let canonical_outcome = serde_json::to_string(&outcome)?;
    let episode_id =
        store.record_episode(session, &features, &action, reward, &canonical_outcome)?;
    let q =
        QLearner::new(config.learning.clone()).update(&store, &state_key, &action, reward, 0.0)?;
    println!(
        "episode = {episode_id}\nstate = {state_key}\naction = {action}\nq_value = {:.4}\nvisits = {}",
        q.value, q.visits
    );
    Ok(())
}

fn usize_as_f64(value: usize) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let converted = value as f64;
    converted
}
