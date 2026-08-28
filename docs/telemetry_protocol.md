# Telemetry protocol v1

GenHOI consumes newline-delimited snapshots embedded in a normal HOI4 log. The
consumer is implemented; a companion mod producer is not claimed until its
field coverage is validated against an installed game version.

## Record

Each accepted line contains the sentinel `GENHOI_TELEMETRY ` followed by one
JSON object:

```json
{
  "schema": 1,
  "sequence": 42,
  "state": {
    "game_hour": 120,
    "country": {},
    "economy": {},
    "wars": [],
    "fronts": [],
    "armies": [],
    "air_regions": [],
    "naval_regions": [],
    "diplomacy": {},
    "strategic_summary": ""
  }
}
```

`state` must match the serialized `GameState` schema. Real records therefore
need all required nested fields; the abbreviated object above is explanatory,
not a valid fixture.

## Consumer guarantees

- ignores unrelated log lines;
- rejects unknown JSON fields and schema versions;
- rejects lines above 1 MiB;
- accepts only monotonically increasing sequence numbers;
- bounds collection sizes and validates finite `[0, 1]` ratios and geometry;
- never sends actions back to HOI4;
- reports offline/degraded/ready health.

Use `genhoi observe --telemetry <path>` for one record or `genhoi run
--telemetry <path>` to follow a growing file. `run` is currently restricted to
observer-only configuration and records accepted decisions into SQLite.

## Producer validation checklist

The producer must be tested with current vanilla game files on both platforms.
For every field, record whether it is directly exposed, derived from exposed
values, unavailable, or hidden by fog of war. Missing data must not be invented.
Measure log flush delay and the overhead of daily/event hooks over a 10-year
hands-off run before declaring the adapter production-ready.

The llama.cpp CLI flags used by the optional reasoner follow the project's
[official CLI documentation](https://github.com/ggml-org/llama.cpp/blob/master/tools/cli/README.md),
including thread/context/token limits and JSON Schema constrained generation.
