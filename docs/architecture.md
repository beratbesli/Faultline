# Faultline Architecture

Faultline is an experimental counterexample discovery and minimization engine for database migrations.

## Core Technical Loop

```text
understand schema
      |
generate valid state
      |
apply migration
      |
observe result
      |
mutate / explore
      |
discover failure
      |
minimize counterexample
      |
verify & replay
```

## System Architecture

```text
               AI Agent (Cursor / Claude / Copilot)
                                |
                    MCP Protocol (JSON-RPC)
                                |
Human Developer ---> CLI ---> FAULTLINE CORE
                                |
    +---------------------------+---------------------------+
    |                           |                           |
Introspection Engine     Search & Generator         Isolated DB Runner
(Live PostgreSQL)     (Relational Synthesizer)      (Ephemeral Clusters)
    |                           |                           |
    +---------------------------+---------------------------+
                                |
                  Counterexample Minimizer
                  (ddmin + Value Shrinking)
                                |
                   Persistence & Replay Store
                       (.faultline/)
```

### 1. Schema Introspection
Authoritative schema discovery directly from PostgreSQL system catalogs (`information_schema`, `pg_catalog`, `pg_constraint`, `pg_attribute`, `pg_type`). Discovers tables, data types, nullability, defaults, primary keys, foreign keys, uniqueness constraints, check constraints, and custom enums.

### 2. State Generation
A relational synthesizer that generates valid candidate states according to schema constraints. It resolves foreign key dependencies via topological sorting (`petgraph`) so parent records are inserted before child records.

### 3. Search Strategies
- **CollisionStrategy**: Case-folding and whitespace variations.
- **PrecisionStrategy**: Fractional and precision boundary numbers.
- **NullabilityStrategy**: Schema-valid NULL generation.
- **BoundaryStrategy**: Min, max, empty, and overflow boundary cases.
- **RandomValidStrategy**: Fuzzing valid states.

### 4. Disposable Isolation
Each experiment runs against an isolated ephemeral PostgreSQL database (`faultline_test_<uuid>`), completely isolating destructive schema modifications.

### 5. Delta-Debugging Minimizer (`ddmin`)
Reduces candidate datasets down to 1-minimal counterexamples by removing non-essential rows in dependency-aware order and shrinking string/numeric values.

### 6. Semantic & Round-trip Verification
Detects semantic data loss and rollback irreversibility (`UP -> DOWN -> COMPARE`).
