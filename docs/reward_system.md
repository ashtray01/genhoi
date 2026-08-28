# Reward-system design

The reward engine compares two normalized snapshots and scores deltas, never a
recommendation before an outcome is observed. Configured terms cover territory,
victory points, casualties, factories, equipment, divisions, encirclements,
wars, supply, failed offensives and manpower exhaustion.

Each term is normalized to a bounded tactical scale before its configurable
weight is applied. Cumulative casualty/factory counters are converted to deltas.
The returned `RewardBreakdown` retains every component for audit and tuning.
