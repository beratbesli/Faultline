# Faultline CLI Reference

## Commands

### `faultline init`
Initializes a new `faultline.yaml` configuration file.
```bash
faultline init [--db-type postgres] [--up-sql <path>] [--down-sql <path>] [--force]
```

### `faultline doctor`
Verifies environment health, PostgreSQL client (`psql`), server (`initdb`), connectivity, and safety rules.
```bash
faultline doctor [--config <path>]
```

### `faultline inspect`
Introspects the target database schema and outputs tables, columns, constraints, and deterministic schema fingerprints.
```bash
faultline inspect [--table <name>] [--json]
```

### `faultline test`
Executes counterexample discovery search against migrations.
```bash
faultline test [--seed <num>] [--experiments <num>] [--time-limit <secs>] [--strategy <name>] [--roundtrip] [--json]
```
- **Exit codes:**
  - `0`: No counterexample discovered within budget.
  - `1`: Counterexample discovered!
  - `2`: Configuration or tool execution error.

### `faultline status`
Shows history and status of search sessions.
```bash
faultline status [--json]
```

### `faultline counterexamples`
Lists all discovered counterexamples stored in `.faultline/`.
```bash
faultline counterexamples [--json]
```

### `faultline replay`
Replays a counterexample reproducibly to verify failure confidence.
```bash
faultline replay <counterexample_id_or_path> [--repeat <count>] [--json]
```

### `faultline report`
Generates human-readable or structured JSON reports of discovered counterexamples.
```bash
faultline report [--id <counterexample_id>] [--json]
```

### `faultline export`
Exports a self-contained reproduction bundle (manifest, schema, seed, migration, reproduce.sh, README.md).
```bash
faultline export <counterexample_id> <output_directory>
```

### `faultline mcp`
Launches the Model Context Protocol (MCP) JSON-RPC stdio server for AI coding agents.
```bash
faultline mcp
```
