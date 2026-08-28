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

## Next

- Phase 3: outcome deltas, configurable reward engine and lightweight Q-values.
- Phase 4: rule-based strategic/operational reasoner and adaptive scheduler.
- Phase 5: optional llama.cpp-compatible reasoner with strict JSON validation.
- Phase 6: lessons, doctrine confidence and after-action review.
- Phase 7: experimentally validated read-only HOI4 telemetry adapter.
- Phase 8: separately gated, rate-limited action executor if validation supports it.
- Phase 9: long-running fixtures, performance budgets and comparative benchmarks.

## Current technical risks

- HOI4 exposes no supported external live-state/action API.
- Mod scripting may not expose division/front detail needed by the normalized model.
- Save formats and script capabilities change between game versions.
- Enemy quantities must preserve fog-of-war semantics and uncertainty.
- UI action execution is fragile across resolution, localization and platform.
- Current heuristic weights are synthetic and need calibration from replay data.
- Cross-platform compilation is designed in, but Linux CI/runtime validation is
  still required on an actual Linux host.
