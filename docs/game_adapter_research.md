# HOI4 game-adapter research

Research date: 2026-08-28. This document separates verified capabilities from
design hypotheses. HOI4's official product page currently lists both Windows
and Linux support, so GenHOI treats both as first-class targets.

## Local installation audit

The development host was checked on 2026-08-28. A legacy HOI4 user-data tree
and March 2025 logs exist, but there is no Steam app manifest `394360` and the
remaining game directory has no vanilla `common/` script tree. Historical
`setup.log` entries confirm that `on_daily_*` actions were registered and
`game.log` lines use the expected timestamp/game-date/source prefix. This is
useful evidence for the consumer parser, but it is not a current install and
cannot validate a companion mod or field coverage. Phase-7 producer validation
therefore remains open rather than relying on stale files.

## Finding

No documented, supported external HOI4 API was found for reading live military
state or issuing player commands. The recommended first real adapter is a
companion HOI4 mod that emits a small, versioned telemetry snapshot through
game-supported scripting/logging, followed by a read-only GenHOI file tailer.
Exactly which fields are script-accessible must be proven against the installed
game version and vanilla scripts before that adapter is implemented.

## Comparison

| Approach | Information | Latency | Difficulty | CPU/I/O | Version robustness | Commands | Invasiveness |
|---|---|---:|---:|---:|---:|---|---|
| Mod + generated telemetry/log | Medium; strong for exposed country/state variables and on-action scopes, uncertain for arbitrary divisions/front geometry | Event-driven to daily tick | Medium | Low if batched; daily hooks must be used cautiously | Medium | Only exposed scripted effects; not general UI commands | Low; explicit mod, checksum impact possible |
| Save-game parsing | Potentially high snapshot coverage | Poor: manual/autosave cadence | High, especially binary/ironman | Bursty disk and parser cost | Low–medium; save compatibility/version changes exist | None | Low/read-only |
| Existing logs | Low unless a companion mod emits known records | Good for logged events | Low | Very low | Medium | None | Low/read-only |
| Console/debug facilities | Useful for research and validation, not a stable protocol | Interactive | Low for experiments, high to automate robustly | Low | Low | Some debug/game effects | Medium; debug-only and unsuitable for production |
| Screen/UI observation | Only visible information; fronts can be inferred geometrically | Seconds | High | Medium–high | Medium; sensitive to UI scale/localization/mods | Possible via ordinary input, but fragile | Low at process level; still controls UI |
| Memory reading/injection | Potentially high | Low | Very high | Variable | Very low | Potentially broad | **Rejected and out of scope** |

## What mod scripting plausibly provides

HOI4 supports `on_actions`: hooks whose effect blocks run when game events occur.
The documented mod structure includes daily hooks, with an explicit warning to
use them cautiously. Script scopes, triggers, variables and effects expose many
country/state facts. The `log` effect can produce a structured sentinel line in
the game log. This is enough to prototype event records such as war starts,
control changes, country-level resources and selected state aggregates.

It does **not** establish that scripts can enumerate every player division,
recover the drawn front line, estimate enemy information beyond game rules, or
issue the same commands as arbitrary UI interaction. Those remain experiments,
not API promises.

### Proposed telemetry protocol experiment

1. A clearly marked single-player companion mod emits newline-delimited records
   with `schema`, `sequence`, `game_hour`, `event`, and a compact payload.
2. Emission occurs on relevant `on_actions` plus a conservative periodic tick,
   not every frame.
3. GenHOI tails only the selected HOI4 log, ignores incomplete lines, validates
   schema/ranges, deduplicates sequence numbers and reports stale health.
4. The adapter merges records into `GameState`; absent values remain unknown or
   conservative rather than fabricated.
5. Validation compares telemetry against the visible UI in six fixed saves on
   both Windows and Linux.

The standard `game.log` may also contain unrelated lines and can be rotated on
restart. A dedicated file is preferable only if the current scripting system is
verified to support it; this document does not assume that it does.

## Save parsing

Save files are attractive for offline replay and ground-truth snapshots. They
are unsuitable as the primary live channel because saving pauses/loads the
game, causes large writes and has coarse latency. HOI4 defines contain an
explicit save compatibility version, evidence that parsers must be versioned.
Text saves can be explored during development; binary and ironman support must
not be promised without a tested decoder and fixtures whose redistribution is
legal. Phase 2 should define an independent GenHOI replay format even if save
import is later added.

## Action options

No action transport is selected in phase 1.

- Mod effects are deterministic and cheap but limited to documented effects;
  using them may change normal gameplay semantics.
- UI input is closer to what a player can do but needs focus checks, coordinate
  calibration, confirmation by observation, a global pause, rate limiting and
  Windows/Linux backends.
- Debug console automation is not a production strategy.

Any executor must remain single-player only, start disabled, perform no action
when adapter health is stale, and record request/result pairs. Memory access,
injection and anti-cheat work are permanently rejected.

## Required phase-7 spikes

1. Inventory current vanilla triggers/effects/scopes for manpower, factories,
   fuel, supply, units, states, wars and control changes.
2. Prove whether compact structured lines can be emitted without localization
   ambiguity and measure log flush latency.
3. Check behavior and user-data/log locations on native Windows and Linux.
4. Measure a daily telemetry hook in a 10-year hands-off run against vanilla.
5. Build golden telemetry fixtures and fuzz malformed/truncated input.
6. Decide separately whether observation coverage is sufficient and whether
   action execution should remain UI-only, mod-only, or unimplemented.

## Sources

- [Paradox product page: supported Windows, macOS and Linux platforms](https://www.paradoxinteractive.com/games/hearts-of-iron-iv/buy)
- [HOI4 Wiki: on actions](https://hoi4.paradoxwikis.com/On_actions)
- [HOI4 Wiki: effects](https://hoi4.paradoxwikis.com/Effects)
- [HOI4 Wiki: scopes](https://hoi4.paradoxwikis.com/Scopes)
- [HOI4 Wiki: variables](https://hoi4.paradoxwikis.com/Variables)
- [HOI4 Wiki: defines, including save version and performance cautions](https://hoi4.paradoxwikis.com/Defines)
- [Paradox developer diary confirming a daily on-action and cautioning its use](https://forum.paradoxplaza.com/forum/developer-diary/hoi4-dev-diary-1-5-2-update-3-and-telemetry.1086632/)

The HOI4 Wiki is community-maintained, so all syntax/capability claims must be
checked against the installed version's game files. The Paradox sources verify
platform support and the historical addition of daily actions, not a public API.
