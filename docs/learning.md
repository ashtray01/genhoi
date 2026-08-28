# Learning design

Learning is intentionally not implemented in phase 1. The planned runtime
learner never updates neural-network weights. It will combine:

1. compact numerical feature vectors from `FrontMetrics`;
2. SQLite episodes containing state, constrained action, outcome and reward;
3. normalized Euclidean or cosine similarity;
4. tabular/contextual Q-values with bounded updates;
5. human-readable proposed lessons whose confidence grows with evidence.

Recorded normalized inputs make learning replayable and auditable. A doctrine
cannot bypass hard tactical safety filters, and proposed lessons do not become
active rules automatically. Training a local LLM remains an optional offline
future experiment.
