# Faultline

> **Generate the data your migration wasn't ready for.**

Most migration tests verify migrations against data developers already thought to include.

Faultline does the opposite.

It generates schema-valid adversarial database states, executes the real migration, observes failures or information loss, and reduces discovered problems into minimal reproducible counterexamples.

---

```text
==================================================
                FAULTLINE REPORT
==================================================
Session ID:              38a67b89c8254f248885ccffce0b091c
Duration:                3.40s
Experiments Executed:    1
Unique States Tested:    1
Counterexamples Found:   1
--------------------------------------------------
COUNTEREXAMPLE FOUND
ID:                      cx_7973356a11784537951b71b765192842
Failure Class:           UNIQUE VIOLATION
Discovery Strategy:      collision
Seed:                    912831
Minimal Reproducing Rows:2
Error:
  [23505] could not create unique index "users_normalized_email_unique" Detail: Key (normalized_email)=(user123) is duplicated.

Minimal Reproducing State:
  Table: users
    - email='Alice@example.com', id=97
    - email='alice@example.com', id=100
==================================================
```

---

## Why Faultline?

Traditional migration tooling asks:
```text
Will this SQL execute on an empty database?
```

Faultline asks the harder question:
```text
What realistic data makes this migration unsafe?
```

Consider this migration:
```sql
ALTER TABLE users ADD COLUMN normalized_email TEXT;
UPDATE users SET normalized_email = LOWER(email);
CREATE UNIQUE INDEX users_normalized_email_unique ON users(normalized_email);
```

On a developer database containing `alice@example.com` and `bob@example.com`, the migration passes. In production, having both `Alice@example.com` and `alice@example.com` will cause a catastrophic migration failure.

Faultline automatically discovers such counterexamples, shrinks them to the minimal reproducing rows, and exports verifiable reproduction bundles.

---

## The Core Loop

```text
Understand Schema ---> Synthesize Valid State ---> Execute Migration
         ^                                               |
         |                                               v
  Replay & Verify <--- Minimize Dataset <--- Observe Failure / Loss
```

- **Predictions suggest risk. Faultline produces evidence.**
- **AI may propose. Faultline must prove.**

---

## Quickstart

### 1. Build and Install
```bash
cargo build --release
cp target/release/faultline ~/.local/bin/
```

### 2. Verify Health
```bash
faultline doctor
```

### 3. Initialize Configuration
```bash
faultline init --up-sql ./migrations/001_up.sql
```

### 4. Inspect Live Schema
```bash
faultline inspect
```

### 5. Run Counterexample Search
```bash
faultline test --experiments 100 --seed 912831
```

### 6. Replay & Export
```bash
# Replay 100 times to verify deterministic confidence
faultline replay <counterexample_id> --repeat 100

# Export standalone reproduction package for bug reports and CI
faultline export <counterexample_id> ./reproduction-package
```

---

## Failure Classes Discovered

1. **Migration Execution Failures**: Unique constraint violations, NOT NULL violations, foreign key mismatches, check constraint errors, invalid type casts, and arithmetic overflow.
2. **Semantic Data Loss**: Migrations that technically succeed but silently destroy information (e.g. `NUMERIC` converted to `INTEGER` truncating `19.99` to `20`).
3. **Collision Introduction**: Trimming, case-folding, and normalization that produce duplicate values.
4. **Irreversible Migrations**: Testing `UP -> DOWN -> COMPARE` to catch rollback data destruction.

---

## Search Strategies

Faultline implements targeted property-based search strategies:
- `collision`: Case-folding and whitespace uniqueness collisions.
- `precision`: Numeric and floating-point precision loss and truncation.
- `nullability`: Generates valid pre-migration NULL values to test `NOT NULL` additions.
- `boundary`: Extreme bounds exploration (`0`, `1`, `-1`, `INT_MAX`, `INT_MIN`, empty strings, overflow values).
- `random`: Fuzzing within strict relational schema constraints.

---

## CI / CD Integration

Faultline returns standard exit codes:
- `0`: No counterexamples discovered within budget.
- `1`: Breaking counterexample discovered.
- `2`: Configuration or database connection error.

Example GitHub Actions workflow:
```yaml
name: Migration Test
on: [pull_request]
jobs:
  faultline:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Run Faultline
        run: faultline test --experiments 500 --seed 42 --json
```

---

## Model Context Protocol (MCP) Integration

Faultline includes a built-in MCP server over `stdio` for AI coding agents:
```bash
faultline mcp
```
Compatible with Claude Code, Cursor, Gemini, and custom agents. Exposes tools for schema inspection, migration analysis, search execution, and counterexample replay without granting raw destructive access to databases.

---

## Documentation

- [Architecture Guide](docs/architecture.md)
- [Configuration Reference](docs/configuration.md)
- [CLI Reference](docs/cli.md)
- [MCP Server Setup](docs/mcp.md)
- [Safety & Isolation Model](docs/safety.md)

---

## License

Apache 2.0 or MIT License. Copyright 2026 Berat Besli.
