# Roadmap

## Completed: phase 0

- Rust 2024 project, MIT license and safe release profile.
- Windows/Linux configuration and build instructions.
- Architecture, integration research and roadmap documentation.

## Completed: phase 1

- Serializable normalized game-state modules.
- `GameAdapter` and observer-only `MockGameAdapter`.
- Six synthetic scenarios.
- Deterministic derived front metrics and recommendations.
- Event types, transition derivation and in-process event bus.
- `genhoi simulate` human/JSON output and `genhoi config`.
- Unit and scenario acceptance tests.

## Completed: phase 2

- Versioned SQLite schema covering sessions, games, states, decisions, actions,
  outcomes, episodes, lessons, doctrines, Q-values, metrics and reports.
- Atomic normalized state/decision recording with observer-only action status.
- Deterministic session loading and `genhoi replay <session>` analysis.
- Native database-path creation and `genhoi db-info` counters.

## Completed: phases 3–6

- Configurable outcome deltas and auditable reward breakdowns.
- Compact feature vectors, cosine episode retrieval and persisted Q-learning.
- Operational front ranking, rule-based strategic reasoner and adaptive scheduler.
- Optional event-driven `llama-cli` GGUF reasoner with JSON Schema and strict
  Rust validation; no online model training.
- Evidence-based proposed lessons, manual activation thresholds and AAR storage.

## Implemented foundation: phases 7–9

- Versioned, bounded, read-only telemetry log adapter with sequence deduplication.
- Runtime orchestration for `observe` and observer-only `run`.
- Shared pause switch, rate limits, dry-run and hard tactical executor gates.
- Cross-platform process RAM/CPU monitoring, latency counters and `benchmark`.
- Windows/Linux CI and tagged prerelease packaging with SHA-256 checksums.

## Next

- Validate and implement the companion HOI4 telemetry producer against an
  installed current game on both Windows and Linux.
- Expand telemetry coverage only for fields proven available in current scripts.
- Keep command execution unavailable until a non-invasive transport passes
  fixture, focus, confirmation and fail-safe testing.
- Run long-duration hands-off experiments with actual telemetry and calibrate
  heuristic/reward weights from recorded evidence.

## Current technical risks

- HOI4 exposes no supported external live-state/action API.
- Mod scripting may not expose division/front detail needed by the normalized model.
- Save formats and script capabilities change between game versions.
- Enemy quantities must preserve fog-of-war semantics and uncertainty.
- UI action execution is fragile across resolution, localization and platform.
- Current heuristic/reward weights are synthetic and need real replay calibration.
- Cross-platform compilation is designed in, but Linux CI/runtime validation is
  still required on an actual Linux host.
