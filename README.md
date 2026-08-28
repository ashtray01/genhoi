# GenHOI

GenHOI is an experimental, local-first, self-learning strategy agent for
single-player Hearts of Iron IV. The current `0.1.0-alpha.3` milestone includes
the local learning pipeline and a read-only telemetry consumer. It does **not**
control HOI4.

The project targets Windows 10/11 x64 and 64-bit Linux. It has no cloud
service, account, outbound analytics, Python runtime, process injection, memory access,
or anti-cheat bypass. The first-launch policy is observer-only; execution and
the local LLM are both disabled in the default configuration.

## Implemented

- normalized serializable `GameState` types;
- synchronous `GameAdapter` boundary and six-scenario `MockGameAdapter`;
- deterministic front metrics and constrained action recommendations;
- in-process event bus and state-derived critical events;
- SQLite session/state/decision recording and deterministic replay;
- configurable rewards, episode similarity and persisted Q-values;
- rule-based and optional local `llama-cli` strategic reasoners;
- doctrine proposals, activation gates and after-action reports;
- read-only telemetry ingestion and an event-driven observer runtime;
- process RAM/CPU/latency monitoring and deterministic benchmarks;
- TOML configuration with native Windows/Linux data paths;
- human-readable and JSON simulation output;
- unit and scenario acceptance tests.

The companion HOI4 telemetry producer and any command transport still require
validation against an installed game. See [the roadmap](docs/roadmap.md).

## Build and run

Install the stable Rust toolchain, then:

```text
cargo build --release
cargo test
cargo run -- simulate
cargo run -- simulate --scenario low-supply
cargo run -- simulate --scenario deep-salient --json
cargo run -- simulate --record
cargo run -- replay <session-id>
cargo run -- db-info
cargo run -- stats
cargo run -- benchmark
cargo run -- observe --telemetry <game.log>
cargo run -- run --telemetry <game.log>
cargo run -- record-outcome <session-id> --action hold --reward 0.5
cargo run -- lessons
cargo run -- doctrine
cargo run -- config
```

Release binaries are `target/release/genhoi.exe` on Windows and
`target/release/genhoi` on Linux. No platform-specific runtime is required.
Tagged alpha versions are built by GitHub Actions and published as prereleases
with Windows and Linux binaries plus SHA-256 checksums.

To use a custom, complete configuration:

```text
genhoi --config config/local.toml simulate
```

Copy `config/default.toml` to `config/local.toml` before editing it. An empty
`paths.data_dir` resolves to `%APPDATA%\GenHOI` on Windows and
`$XDG_DATA_HOME/genhoi` or `~/.local/share/genhoi` on Linux.

## Safety status

`simulate` only analyzes synthetic data and prints recommendations. Even the
mock adapter rejects actions until its execution gate is explicitly enabled in
code. The real executor will not be implemented before an adapter can be
observed, replayed and independently validated.

## Optional local LLM

Install a current `llama.cpp` `llama-cli`, place a local 0.5B–1.5B Q4 GGUF
model under `models/`, and explicitly enable `[llm]` in a local configuration.
GenHOI invokes it only when the adaptive scheduler requests strategic reasoning.
Output is constrained with llama.cpp JSON Schema and validated again in Rust.
No model is bundled or downloaded automatically; rule-based operation remains
the default.

## Documentation

- [Architecture](docs/architecture.md)
- [HOI4 adapter research](docs/game_adapter_research.md)
- [Telemetry protocol](docs/telemetry_protocol.md)
- [Learning design](docs/learning.md)
- [Reward design](docs/reward_system.md)
- [Roadmap and risks](docs/roadmap.md)

## License

MIT.
