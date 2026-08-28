# Architecture

## Scope of version 0.1.0-alpha.3

The implementation includes the local pipeline through doctrine generation,
plus a versioned read-only telemetry consumer. Real HOI4 telemetry production
and command transport remain unverified and therefore disabled.

## Data flow

```text
HOI4 or simulator
      |
      v
GameAdapter -- observe/execute/health
      |
      v
normalized GameState
      |
      +--> event derivation --> EventBus --> adaptive scheduler (future)
      |
      +--> FrontMetrics --> deterministic planner
                                  |
                                  v
                          constrained GameAction
                                  |
                   safety gate / observer-only default
                                  |
                                  v
                           GameAdapter::execute
```

The adapter owns every game-specific assumption. Core modules consume only
serializable Rust structures and therefore work unchanged with mock telemetry,
save snapshots, mod telemetry or a future screen observer.

## Module boundaries

- `state`: compact observations, not raw screenshots or engine internals.
- `adapter`: fallible observation/action boundary with an explicit health state.
- `metrics`: pure calculations with no I/O, clock, randomness or LLM calls.
- `planner`: a small constrained `GameActionKind` vocabulary.
- `event`: state transition/event detection and non-blocking in-process fan-out.
- `simulation`: end-to-end deterministic scenario runner and output model.
- `config`: complete TOML settings and native Windows/Linux path resolution.
- `memory`, `learning`, `reward`: SQLite episodes, similarity, Q-values and outcomes.
- `reasoner`, `operational`, `scheduler`, `runtime`: event-driven three-level AI.
- `doctrine`: evidence-based proposals and after-action reviews.
- `telemetry`: bounded, versioned, read-only HOI4 log ingestion.
- `safety`, `performance`: execution gates and resource/latency monitoring.

All normalized structs use `serde`, so a later replay recorder can persist the
exact inputs and outputs without changing core types.

## Deterministic risk model

`FrontMetrics::calculate` clamps normalized inputs to `[0, 1]`, protects ratios
against zero denominators, and calculates:

- force ratio;
- supply and air-support scores;
- salient ratio and risk;
- reserve strength and equipment shortage;
- offensive/defensive potential and front stability;
- encirclement risk.

Encirclement risk includes an explicit interaction term when all of these hold:
salient ratio at least 4, supply below 55%, and pressure above 70% on both
shoulders. This encodes the fact that the conjunction is more dangerous than a
linear average. It is a transparent heuristic to calibrate with replay data,
not a claim about the HOI4 engine's internal formula.

## Safety invariants

1. Default configuration is observer-only, dry-run and executor-off.
2. The LLM is disabled and will never be required for tactical safety rules.
3. An adapter must reject execution unless its own gate is enabled.
4. Recommendations are enum values; free text is never executable.
5. An interval can return no more than the configured action limit.
6. No process injection, memory manipulation, DLL injection or multiplayer
   automation will be added.

## Cross-platform decisions

The core uses stable Rust and synchronous standard-library primitives. This
keeps both Windows and Linux builds small and avoids an idle async runtime.
Paths are `PathBuf`, never manually concatenated strings. Data defaults follow
`APPDATA` on Windows and the XDG data convention on Linux. A future HOI4
adapter will resolve game-specific directories separately from GenHOI's data.

## Future extension points

- The telemetry adapter implements `GameAdapter` without changing metrics.
- `StrategicReasoner` consumes a compact summary and returns validated JSON.
- SQLite recording serializes states/actions at the simulation seam.
- Replay can feed recorded observations through `MockGameAdapter::with_observations`.
- Q-values and retrieved episodes rank only constrained candidate actions;
  hard safety filters remain deterministic and run afterward.
