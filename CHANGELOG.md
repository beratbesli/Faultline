# Changelog

All notable changes to Faultline will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-09-07

### Added
- **PostgreSQL Schema Introspection**: Authoritative live database discovery inspecting tables, columns, primary keys, foreign keys, unique constraints, check constraints, indexes, and custom enums.
- **Deterministic State Generation**: Schema-aware relational data synthesizer respecting foreign key dependencies, primary keys, and pre-migration validity.
- **Search Strategies**:
  - `collision`: Detects case-folding and whitespace uniqueness collisions (e.g. `LOWER(email)` + `UNIQUE index`).
  - `precision`: Discovers decimal and numeric rounding/truncation data loss.
  - `nullability`: Discovers schema-valid null values breaking incoming `NOT NULL` constraints.
  - `boundary`: Extreme bounds exploration (`0`, `1`, `-1`, `INT_MAX`, `INT_MIN`, empty strings, max-length strings).
  - `random`: Fuzzing within strict schema validity constraints.
- **Migration Runners**:
  - Direct SQL migration execution.
  - Command-based runner with configurable timeouts and environment variable injection (`DATABASE_URL`).
  - Migration SQL analyzer providing automatic strategy recommendations.
- **Safety & Isolation**:
  - Automatic creation and teardown of disposable ephemeral test databases (`faultline_test_<uuid>`).
  - Production guard checking database names and known public cloud hosting providers.
- **Counterexample Minimization**:
  - Delta-debugging algorithm (`ddmin`) performing foreign-key-aware row reduction.
  - Value shrinking algorithm reducing complex strings and numbers down to minimal reproduces.
- **Semantic & Round-trip Checks**:
  - Semantic data loss detection across column data alterations.
  - Roundtrip testing executing `UP -> DOWN -> COMPARE` to detect irreversible migrations.
- **Deterministic Replay & Persistence**:
  - Reproducible random seeds (`--seed`).
  - Standalone reproduction bundles (`manifest.json`, `schema.sql`, `seed.sql`, `migration_up.sql`, `reproduce.sh`, `README.md`).
  - Repeatable confidence verification via `faultline replay <target> --repeat <n>`.
- **Model Context Protocol (MCP)**:
  - Standard JSON-RPC 2.0 stdio server (`faultline mcp`) exposing structured tools to AI agents safely.
- **Comprehensive CLI**: `init`, `doctor`, `inspect`, `test`, `status`, `resume`, `minimize`, `replay`, `report`, `counterexamples`, `export`, and `mcp`.
- **Integration Fixtures**: Real end-to-end integration fixtures for case collisions, precision loss, not-null constraints, and rollback irreversibility.
