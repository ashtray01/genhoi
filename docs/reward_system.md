# Reward-system design

The future reward engine will compare two normalized snapshots and score deltas,
not reward a recommendation before an outcome is observed. Configured weights
already reserve the intended signals: territory, enemy losses, factory gain,
own manpower/equipment loss, supply deficit and encirclement.

Before phase 3, every term needs a precise unit and normalization window. Large
strategic terminal rewards (war won/lost) must not drown out tactical feedback,
and cumulative counters must be converted to deltas. Tests will include no-op,
successful breakthrough, failed offensive and destroyed-division outcomes.
