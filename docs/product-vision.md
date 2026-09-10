# Product vision and boundaries

## Decision

Faultline is an evidence-producing database migration tester. It synthesizes
schema-valid adversarial states, executes the real migration in isolation,
detects execution failures or semantic data loss, and minimizes a failure to a
replayable counterexample.

The experimental result is authoritative. Heuristics and optional AI clients
may propose candidates, but they must not classify a migration as safe or
unsafe without a reproducible experiment.

## Core workflow

1. Inspect and fingerprint the source schema.
2. Generate relationally valid boundary, collision, nullability, precision, or
   seeded-random data.
3. Run the configured migration in an isolated database.
4. Compare execution results and declared semantic invariants.
5. Minimize failures while preserving the observed behavior.
6. Save a deterministic replay bundle and human/JSON report.

## Safety boundaries

- Fail closed if the target identity or schema fingerprint is unexpected.
- Treat migration commands, configuration, databases, and replay bundles as
  untrusted input.
- Prefer disposable containers or databases with least-privilege credentials.
- Never infer production safety from a finite search budget; "no
  counterexample found" is not a proof of correctness.
- Keep MCP as a bounded adapter over the same core operations. It must not
  expose arbitrary SQL, shell execution, or credentials.

## Near-term scope

- PostgreSQL-first migration execution and schema-aware generation.
- Deterministic seeds, resumable experiments, ddmin reduction, replay, and
  export.
- CI-friendly exit codes and structured reports.
- Controlled local MCP stdio integration.

Other database engines, a hosted service, distributed execution, and
AI-generated migration repair are possible future work, not current product
claims.

## Non-goals

- Replacing backups, review, staging tests, or production observability.
- Executing against production databases.
- Proving the absence of every migration defect.
- Letting an AI model make unverified safety decisions.

This concise decision record replaces the original implementation prompt. The
living architecture, CLI, configuration, MCP, and safety details belong in the
focused documents under `docs/`.
