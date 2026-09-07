# FAULTLINE — MASTER IMPLEMENTATION PROMPT

You are the lead systems engineer responsible for designing and implementing **Faultline**, an open-source developer tool that automatically discovers the smallest realistic database state capable of breaking a database migration or exposing semantic data loss.

Repository:

https://github.com/beratbesli/Faultline

You are responsible for the entire engineering lifecycle:

- architecture
- implementation
- test infrastructure
- database integration
- counterexample generation
- migration execution
- minimization
- safety
- CLI
- structured API
- optional AI/MCP integration
- documentation
- Git history
- release readiness

Do not stop at architecture notes, scaffolding, TODOs, interfaces, mock implementations, or proof-of-concept output.

Build a real, working first release.

---

# 1. PROJECT MISSION

Faultline answers this question:

> What valid database state can make this migration fail, lose information, become irreversible, or break application compatibility?

Traditional migration tooling usually asks:

```text
Will this SQL execute?
```

Faultline asks a harder question:

```text
What realistic data makes this migration unsafe?
```

Example migration:

```sql
ALTER TABLE users
ADD COLUMN normalized_email TEXT;

UPDATE users
SET normalized_email = LOWER(email);

CREATE UNIQUE INDEX users_normalized_email_unique
ON users(normalized_email);
```

A normal test database containing:

```text
alice@example.com
bob@example.com
```

passes.

Production may contain:

```text
Berat@example.com
berat@example.com
```

and the migration fails.

Faultline must automatically search for such counterexamples.

Expected result:

```text
FAULTLINE COUNTEREXAMPLE

Migration:
041_normalize_email.sql

Failure:
unique constraint violation

Minimal reproducing state:

users
--------------------------------
id     email
41     Berat@example.com
92     berat@example.com

Rows required:
2

Reproduction:
100 / 100 successful failures
```

The user should not need to think of the edge case first.

Faultline should discover it.

---

# 2. CORE PRODUCT PHILOSOPHY

Faultline is NOT a migration linter.

Faultline is NOT merely static SQL analysis.

Faultline is NOT an AI wrapper.

Faultline is an experimental counterexample discovery system.

Its core loop is:

```text
understand schema
      |
generate valid state
      |
apply migration
      |
observe result
      |
mutate state
      |
run again
      |
discover failure
      |
minimize counterexample
      |
verify repeatedly
```

The central principle is:

> Predictions suggest risk. Faultline produces evidence.

Or more simply:

> AI may propose. Faultline must prove.

---

# 3. AI MUST BE OPTIONAL

Faultline must work completely without AI.

The deterministic core must not require:

- OpenAI
- Claude
- Gemini
- cloud inference
- LLM API keys
- internet access

A normal developer must be able to run:

```bash
faultline test
```

and receive real counterexamples.

AI may later improve:

- semantic edge-case proposals
- column relationship hypotheses
- domain-specific generators
- report explanations
- prioritization of mutation strategies

But AI must never be required for correctness.

The architecture should conceptually be:

```text
                 AI Agent
                    |
                MCP / API
                    |
Human -> CLI -> FAULTLINE CORE
                    |
      --------------------------------
      |               |              |
 Schema Analyzer   Generator      Executor
      |               |              |
      -------- Counterexample Engine --
```

Humans and AI agents must invoke the same underlying engine.

---

# 4. INITIAL RELEASE SCOPE

Build a focused and reliable v0.1.

Primary database:

- PostgreSQL

Primary environment:

- Linux
- Docker / Docker Compose

The first release must support:

- PostgreSQL schema discovery
- migration discovery
- SQL migration execution
- configurable migration commands
- schema-aware data generation
- constraint-respecting data generation
- mutation-based search
- migration failure detection
- semantic data-loss checks
- round-trip migration checks
- counterexample minimization
- deterministic replay
- reproducible seeds
- experiment persistence
- resumable sessions
- CLI
- machine-readable JSON output
- realistic integration fixtures
- optional MCP-compatible tool interface

Do NOT prematurely add five database engines.

Build PostgreSQL extremely well first.

Architect the system so future adapters can support:

- MySQL
- MariaDB
- SQLite
- CockroachDB
- SQL Server
- MongoDB schema migrations
- application-level migration frameworks

---

# 5. IMPORTANT DIFFERENTIATOR

Faultline must not simply:

```text
scan migration
detect suspicious SQL
print warning
```

Example of insufficient behavior:

```text
WARNING:
LOWER(email) combined with UNIQUE may cause collisions.
```

That is useful static analysis, but it is not Faultline's primary value.

Faultline should instead produce:

```text
COUNTEREXAMPLE FOUND

Before migration:

users:
  id=1
  email="Berat@example.com"

users:
  id=2
  email="berat@example.com"

Migration result:
FAILED

Error:
duplicate key violates unique constraint

Reproduction seed:
812947

Counterexample size:
2 rows
```

Faultline must produce executable evidence.

---

# 6. TYPES OF FAILURES

Faultline should be designed to discover multiple migration failure classes.

At minimum:

## 6.1 Migration execution failure

Examples:

- UNIQUE violations
- NOT NULL violations
- CHECK violations
- foreign-key violations
- invalid casts
- arithmetic overflow
- malformed transformed values
- invalid enum conversions

---

## 6.2 Semantic data loss

Migration succeeds but information is destroyed.

Example:

```text
before:
price = 19.99

after:
price = 19
```

Migration technically succeeds.

Faultline should detect meaningful data change when configured invariants or transformation analysis indicate that information should have been preserved.

---

## 6.3 Collision introduction

Example:

```text
"Berat"
"berat"
```

become:

```text
"berat"
"berat"
```

after normalization.

Other examples:

- trimming
- Unicode normalization
- case folding
- numeric rounding
- timestamp truncation

---

## 6.4 Irreversible migration

Test:

```text
Schema/Data V1
     |
   migrate up
     |
Schema/Data V2
     |
 migrate down
     |
Schema/Data V1'
```

Compare:

```text
V1 vs V1'
```

Faultline should discover datasets where rollback cannot reconstruct the original state.

---

## 6.5 Old/new application compatibility

Design the architecture for future testing of:

```text
App V1
App V2
Migration
Shared Database
```

Example sequence:

```text
V1 writes
migration step executes
V2 reads
V2 writes
V1 reads
```

A migration may be individually valid but break rolling deployments.

This does not need to be fully implemented in the earliest milestone, but architecture must leave a clear path.

---

# 7. SCHEMA MODEL

Build a rich internal schema model.

At minimum represent:

```text
DatabaseSchema
Table
Column
PrimaryKey
ForeignKey
UniqueConstraint
CheckConstraint
Index
Enum
DefaultValue
GeneratedColumn
Sequence
```

Inspect real PostgreSQL metadata.

Do not rely only on parsing migration SQL.

The live database schema must be considered authoritative when testing migrations.

Support introspection of:

- PostgreSQL data types
- nullability
- defaults
- PK/FK relationships
- uniqueness
- CHECK expressions where possible
- enum values
- sequences
- generated columns
- indexes

Use this model to drive valid state generation.

---

# 8. VALID STATE GENERATION

Generated data should generally satisfy the pre-migration schema.

Faultline should not claim to find a migration bug if the generated input database was already invalid.

Generators should respect:

- NOT NULL
- primary keys
- foreign keys
- uniqueness
- enum constraints
- type bounds
- basic CHECK constraints when feasible

Build deterministic primitive generators for common PostgreSQL types:

```text
integer
bigint
numeric
decimal
real
double precision
boolean
text
varchar
char
uuid
date
timestamp
timestamptz
json
jsonb
enum
arrays
```

The initial implementation may support a focused subset first, but structure the generator system cleanly.

---

# 9. EDGE-CASE GENERATION

This is critical.

Do not only generate random ordinary values.

Faultline should deliberately explore edge-case classes.

For text:

```text
empty string
single-character
long string
leading/trailing spaces
different case
Unicode
normalization equivalents
combining characters
similar prefixes
similar suffixes
duplicate-like values
```

For numbers:

```text
0
1
-1
min
max
near boundaries
decimal precision boundaries
values that truncate or round
```

For time:

```text
epoch
day boundary
month boundary
year boundary
DST-sensitive values
timezone offsets
precision differences
```

For nullable columns:

```text
NULL
non-NULL
mixed sets
```

For relationships:

```text
one-to-one
one-to-many
orphan-like edge cases that remain schema-valid
shared references
duplicate semantic identity
```

The generator engine must be extensible.

---

# 10. GENERATOR ARCHITECTURE

Define clear abstractions such as:

```text
ValueGenerator
RowGenerator
TableGenerator
DatabaseGenerator
MutationStrategy
GenerationStrategy
GenerationSeed
```

Example conceptual interface:

```text
generate(schema, seed)
mutate(state, strategy, seed)
shrink(state)
```

Do not hardcode all generation logic into migration execution code.

---

# 11. PROPERTY-BASED SEARCH

Faultline should use property-based testing ideas.

The basic property is often:

```text
Given any schema-valid pre-migration database state,
migration should satisfy the configured migration invariant.
```

Examples of invariants:

```text
migration executes successfully
migration preserves row identity
migration preserves configured columns
rollback restores original state
old application remains compatible
```

The system should repeatedly generate and mutate candidate states.

Support deterministic random seeds.

Example:

```bash
faultline test --seed 912831
```

The same seed must produce the same generation behavior whenever reasonably possible.

This is critical for reproducibility.

---

# 12. SEARCH ENGINE

Do not depend exclusively on naive random fuzzing.

Implement a structured search strategy.

Possible stages:

```text
1. schema-derived edge cases
2. constraint-boundary cases
3. transformation-derived hypotheses
4. random valid generation
5. mutation of promising states
6. combination search
```

Track which strategies produce useful failures.

Persist experiment metadata.

Do not repeat identical experiments unnecessarily.

Use state fingerprints to detect duplicates.

---

# 13. MIGRATION ANALYSIS

Faultline may analyze migration SQL to prioritize generation strategies.

For example:

```sql
LOWER(email)
```

may suggest:

```text
case-collision generation
Unicode case behavior
```

```sql
CAST(price AS INTEGER)
```

may suggest:

```text
fractional values
large values
negative values
```

```sql
SET NOT NULL
```

may suggest:

```text
NULL-bearing rows
```

```sql
VARCHAR(255) -> VARCHAR(32)
```

may suggest:

```text
length boundaries
```

This analysis should be treated as strategy selection.

It must not replace execution.

A migration is only considered broken when Faultline demonstrates it.

---

# 14. MIGRATION EXECUTION

Support multiple migration execution modes.

At minimum:

```text
raw SQL file
shell command
```

Example configuration:

```yaml
migration:
  up:
    command: "./migrate-up.sh"

  down:
    command: "./migrate-down.sh"
```

Also support direct SQL where practical:

```yaml
migration:
  up_sql: "./migrations/041.sql"
```

Future adapters may integrate with:

- Flyway
- Liquibase
- Alembic
- Django migrations
- Rails migrations
- Prisma
- Drizzle
- Knex

Do not require framework-specific support for v0.1.

Generic command execution is enough for compatibility.

---

# 15. ISOLATED TEST DATABASES

Never test destructive migrations directly against an arbitrary user database.

Use disposable isolated databases.

Preferred lifecycle:

```text
baseline database
      |
create isolated test database
      |
populate generated candidate
      |
capture before-state fingerprint
      |
run migration
      |
capture after-state
      |
evaluate
      |
destroy isolated environment
```

Dockerized PostgreSQL support is strongly preferred for integration testing.

When possible, optimize using:

- database templates
- snapshot cloning
- transaction rollback
- efficient dump/restore

Correctness comes before speed.

---

# 16. SAFETY

Faultline is intentionally executing database migrations.

Safety must be a first-class feature.

Implement protections such as:

- explicit configuration
- production-target detection
- connection fingerprinting
- database-name checks
- destructive-operation warnings
- isolated database requirement by default
- explicit override for non-isolated targets
- never silently modifying unknown databases
- no automatic operation against production-looking endpoints

The safest default should be:

> Refuse to run unless Faultline can verify it is working against an isolated disposable environment.

Provide clear errors instead of risky convenience.

---

# 17. COUNTEREXAMPLE MINIMIZATION

Finding a failure is not enough.

Faultline must shrink the dataset to the smallest practical reproducer.

Example:

```text
Failure discovered with:
50,000 rows
```

Reduce:

```text
50,000
25,000
12,500
...
8
4
2
```

Goal:

```text
users:
  {email: "Berat@example.com"}
  {email: "berat@example.com"}
```

Use techniques such as:

- delta debugging
- ddmin
- hierarchical reduction
- row removal
- table removal
- column simplification
- value shrinking
- dependency-aware shrinking

The minimizer should aim for a defensible locally minimal or 1-minimal counterexample.

Do not falsely claim global minimality unless guaranteed.

---

# 18. DEPENDENCY-AWARE MINIMIZATION

Rows cannot always be removed independently.

Example:

```text
users.id=174
     |
orders.user_id=174
```

Removing the user while keeping the order may violate the original schema.

Build a dependency graph using:

- foreign keys
- primary keys
- unique constraints
- table relationships

The minimizer should remove dependency-consistent groups where necessary.

For example:

```text
Remove user 174
   |
also remove dependent orders
   |
test migration
```

This should happen deterministically.

---

# 19. VALUE SHRINKING

Do not only shrink row count.

Shrink values.

Example:

```text
"ABCDEFGHIJKLMNOPQRSTUVWXYZBerat@example.com"
```

may reduce to:

```text
"A@example.com"
```

or eventually:

```text
"A"
```

if the failure persists.

For numeric counterexamples:

```text
9223372036854775807
```

could shrink toward the smallest boundary-triggering value.

Value shrinkers should be type-specific.

---

# 20. SEMANTIC DATA COMPARISON

Migration success is not sufficient.

Provide mechanisms to compare before and after state.

At minimum:

```text
row counts
primary-key identity
column values
configured invariants
checksums
```

Allow configuration of fields expected to change.

Example:

```yaml
invariants:
  preserve:
    - users.id
    - users.created_at

  ignore:
    - users.normalized_email
```

Do not blindly report expected transformation as corruption.

---

# 21. ROUND-TRIP TESTING

Support:

```bash
faultline test --roundtrip
```

Workflow:

```text
generate V1 state
      |
migration UP
      |
migration DOWN
      |
compare to V1
```

Report minimal states that violate reversibility.

Example:

```text
ROLLBACK COUNTEREXAMPLE

Before:
price = 19.99

After UP + DOWN:
price = 19.00

Information lost:
0.99
```

---

# 22. EXPERIMENT MODEL

Every test execution should become a structured experiment.

Persist at least:

```text
experiment id
timestamp
seed
database fingerprint
schema fingerprint
migration fingerprint
generation strategy
mutation strategy
candidate state fingerprint
migration exit status
migration error
before-state fingerprint
after-state fingerprint
duration
result
```

Never rely only on console logs.

Experiments must be inspectable later.

---

# 23. SESSION PERSISTENCE

Searches may take a long time.

Faultline sessions must be resumable.

Support concepts such as:

```text
TestSession
Experiment
Counterexample
ReductionSession
```

Commands should eventually support:

```bash
faultline status
faultline resume
faultline experiments
```

An interrupted run must not discard already discovered results.

---

# 24. CLI

Build a polished CLI.

Suggested commands:

```bash
faultline init
faultline doctor
faultline inspect
faultline test
faultline status
faultline resume
faultline minimize
faultline replay
faultline report
faultline counterexamples
```

Example:

```bash
faultline doctor
```

checks:

```text
configuration
PostgreSQL connectivity
isolation safety
migration command
schema visibility
Docker availability
workspace state
```

---

## Test

```bash
faultline test
```

should:

1. inspect schema
2. establish isolated environment
3. generate candidate states
4. execute migration
5. evaluate results
6. persist experiments
7. stop or continue according to configuration
8. automatically minimize discovered failures where appropriate

---

## Structured output

Support:

```bash
faultline status --json
faultline report --json
```

Machine consumers must not scrape human-formatted terminal output.

---

# 25. REPORTING

Generate useful human-readable reports.

Example:

```text
FAULTLINE REPORT
────────────────────────────────────

Migration:
041_normalize_email.sql

Failure class:
UNIQUE COLLISION

Discovery strategy:
case-collision

Seed:
912831

Original failing candidate:
2,184 rows

Minimal counterexample:
2 rows

users
────────────────────────────────────
id   email
41   Berat@example.com
92   berat@example.com

Migration result:
FAILED

PostgreSQL error:
duplicate key violates unique constraint

Reproduction:
100 / 100

Suggested interpretation:
LOWER(email) maps two distinct pre-migration values
to the same normalized value.
```

The final interpretation may be rule-based or optionally AI-enhanced.

Clearly distinguish:

```text
experimentally observed
```

from:

```text
inferred explanation
```

---

# 26. REPLAY

Counterexamples must be reproducible.

Provide:

```bash
faultline replay <counterexample>
```

A counterexample artifact should contain enough data to reconstruct the failing state.

Conceptually:

```text
counterexample/
├── manifest.json
├── schema.sql
├── seed.sql
├── migration/
├── metadata.json
├── reproduce.sh
└── README.md
```

The exact format is your responsibility.

Avoid embedding secrets.

---

# 27. EXPORT

Support eventual:

```bash
faultline export <counterexample> ./output
```

The exported reproduction should be usable independently when feasible.

A developer should be able to attach it to:

- GitHub issue
- bug report
- CI artifact
- pull request
- migration review

---

# 28. CI INTEGRATION

Faultline should eventually work in CI.

Example:

```yaml
- name: Test database migration
  run: faultline test --budget 500
```

Possible useful controls:

```text
experiment budget
time budget
seed
failure classes
maximum generated rows
strategy selection
```

CI mode must produce deterministic structured output and meaningful exit codes.

For example:

```text
0 = no counterexample discovered
1 = counterexample discovered
2 = tool/configuration failure
```

Choose exact semantics carefully and document them.

---

# 29. TEST BUDGETS

Counterexample discovery is potentially unbounded.

Support explicit resource budgets.

Examples:

```bash
faultline test --experiments 1000
faultline test --time-limit 10m
```

Track:

```text
experiments executed
unique states tested
migrations executed
counterexamples found
time spent
```

Do not pretend that failure absence proves universal safety.

Report instead:

```text
No counterexample found within:
5,000 experiments
12 strategies
8m 42s
```

Never claim:

```text
Migration is guaranteed safe.
```

unless mathematically justified.

---

# 30. SEARCH STRATEGIES

Design strategy interfaces.

Possible strategies:

```text
BoundaryStrategy
NullabilityStrategy
CollisionStrategy
UnicodeStrategy
NumericPrecisionStrategy
TimestampStrategy
ConstraintStrategy
RandomValidStrategy
MutationStrategy
TransformationAwareStrategy
```

Strategies should be pluggable.

The system should eventually allow:

```bash
faultline test --strategy collision
```

or configuration equivalents.

---

# 31. SQL TRANSFORMATION HINTS

Create a lightweight migration analyzer that detects operations useful for guiding generation.

Examples:

```text
LOWER
UPPER
TRIM
CAST
ROUND
COALESCE
SUBSTRING
CONCAT
JSON extraction
SET NOT NULL
ADD UNIQUE
type changes
column narrowing
enum changes
foreign-key introduction
```

Do not attempt to build a perfect SQL theorem prover.

The analyzer's purpose is to prioritize experiments.

Actual database execution remains authoritative.

---

# 32. POSTGRESQL-FIRST QUALITY

PostgreSQL support must feel native.

Use PostgreSQL's own metadata and error information.

Capture:

- SQLSTATE
- constraint names
- table names
- column names
- error details where available

Structured error classification should allow Faultline to distinguish:

```text
unique violation
foreign key violation
check violation
not null violation
invalid text representation
numeric out of range
serialization issues
timeout
migration tool failure
```

Do not parse human error strings when PostgreSQL exposes structured error codes.

---

# 33. MIGRATION FRAMEWORK AGNOSTIC DESIGN

Faultline must not be tied to one migration framework.

Core abstraction should conceptually resemble:

```text
MigrationRunner
    |
    +-- SQLRunner
    +-- CommandRunner
    +-- Future AlembicRunner
    +-- Future PrismaRunner
```

The first release should provide generic SQL and command execution.

That will cover many ecosystems immediately.

---

# 34. OPTIONAL AI / MCP INTEGRATION

After the deterministic core is stable, expose controlled operations for coding agents.

Potential MCP tools:

```text
get_project_status
inspect_schema
inspect_migration
list_strategies
start_search
get_search_status
list_counterexamples
inspect_counterexample
minimize_counterexample
replay_counterexample
generate_report
```

Do NOT expose unrestricted SQL execution to an AI agent unless explicitly sandboxed.

AI operations should go through Faultline's safety model.

Example agent workflow:

```text
User:
Find whether this migration can destroy data.

AI:
inspect migration
inspect schema
ask Faultline to run precision/collision strategies
inspect discovered counterexample
request minimization
explain experimentally verified failure
```

Again:

> AI proposes. Faultline proves.

---

# 35. HUMAN USABILITY

Faultline must be equally useful without AI.

A normal human workflow should be simple:

```bash
faultline init

faultline doctor

faultline test

faultline report
```

The user should not need to understand fuzzing internals to use the tool.

Provide sensible defaults.

Advanced options may remain available.

---

# 36. OPTIONAL TUI

A terminal UI is optional.

Only implement it after the core works reliably.

Potential views:

```text
Project
Schema
Migration
Search
Experiments
Counterexamples
Minimization
Reports
```

Example:

```text
┌ Faultline ───────────────────────────────────────┐
│ Migration: 041_normalize_email.sql              │
│                                                  │
│ Experiments             3,821                    │
│ Unique states           3,190                    │
│ Counterexamples             4                    │
│                                                  │
│ Current strategy: Collision                     │
│                                                  │
│ Best counterexample:                             │
│ 2 rows                                           │
│ UNIQUE violation                                 │
└──────────────────────────────────────────────────┘
```

Do not let TUI work delay the core engine.

---

# 37. PROJECT CONFIGURATION

Create a clear configuration format.

Example:

```yaml
version: 1

project:
  name: normalize-email

database:
  type: postgres
  url_env: DATABASE_URL

migration:
  up:
    command: "./scripts/migrate-up.sh"

  down:
    command: "./scripts/migrate-down.sh"

testing:
  experiments: 5000
  seed: 812947

checks:
  migration_success: true
  roundtrip: true
```

Credentials must never be written into generated config files by default.

Use environment variable references.

---

# 38. SAMPLE PROJECT

Create a real Docker Compose integration fixture.

At minimum:

```text
PostgreSQL
migration
seed schema
intentional migration bug
```

Construct several migrations demonstrating different failure classes.

For example:

### Fixture A — Case collision

```text
LOWER(email)
+
UNIQUE index
```

Expected minimal counterexample:

```text
A@example.com
a@example.com
```

### Fixture B — Precision loss

```text
NUMERIC -> INTEGER
```

Expected counterexample:

```text
19.99
```

### Fixture C — NOT NULL introduction

Migration assumes no NULLs.

Faultline should generate a schema-valid NULL-containing state before migration.

### Fixture D — rollback loss

UP and DOWN technically succeed but do not reconstruct original data.

These fixtures prove Faultline is real.

---

# 39. TESTING REQUIREMENTS

Provide:

- unit tests
- property tests where appropriate
- PostgreSQL integration tests
- generator tests
- minimizer tests
- migration runner tests
- schema inspection tests
- counterexample replay tests
- safety tests
- persistence/resume tests

Test deterministic seeds.

Test failure handling.

Test interrupted sessions.

Test malformed configuration.

---

# 40. TECHNOLOGY CHOICE

Choose technology based on engineering suitability.

Strong candidates:

- Rust
- Go

Important requirements:

- reliable CLI
- PostgreSQL driver quality
- concurrency support
- static/simple deployment
- structured serialization
- robust testing
- cross-platform potential

Inspect the repository before choosing.

If the repository already contains a coherent stack, preserve it unless there is a strong reason to change.

Document major architectural decisions.

---

# 41. ARCHITECTURE

Maintain clear separation between:

```text
schema
migration
generation
search
mutation
execution
comparison
minimization
persistence
reporting
CLI
MCP/API
safety
```

Avoid giant modules.

Avoid god objects.

Avoid mixing SQL execution with terminal rendering.

The CLI must be a thin layer over reusable core APIs.

---

# 42. PERFORMANCE

Migration execution may be expensive.

Design for:

- efficient database reset
- experiment caching
- state fingerprints
- duplicate-state detection
- incremental search
- hierarchical generation
- targeted strategies
- reusable database templates

Do not introduce unsafe parallel execution early.

Parallelism should only be added when each experiment is isolated.

Correctness before speed.

---

# 43. DETERMINISM

Where possible:

```text
same schema
same migration
same seed
same Faultline version
```

should produce equivalent search behavior.

Record:

```text
Faultline version
PostgreSQL version
seed
strategy
migration hash
schema hash
```

inside reports.

Reproducibility is a core product feature.

---

# 44. GIT PROTOCOL — MANDATORY

Git discipline is a hard requirement.

Repository:

https://github.com/beratbesli/Faultline

Before changing anything, inspect the repository:

```bash
git status
git branch --show-current
git log --oneline --decorate -n 10
git remote -v
```

Never destroy unrelated work.

Never use destructive history rewriting.

Never force-push.

Never reset existing user work merely to simplify implementation.

---

# 45. COMMIT AFTER EVERY COMPLETED LOGICAL STEP

This is mandatory.

Do NOT implement the whole project and commit everything at the end.

Every meaningful completed step must be committed immediately.

Examples:

```text
chore: initialize Faultline project structure

feat(config): add project configuration

feat(postgres): add PostgreSQL schema introspection

feat(migration): add command-based migration runner

feat(generator): add schema-aware primitive generators

feat(strategy): add collision generation strategy

feat(search): implement deterministic experiment engine

feat(minimize): add dependency-aware row reduction

feat(roundtrip): detect irreversible migrations

feat(report): add structured counterexample reports

feat(cli): implement test and report commands

feat(mcp): expose controlled Faultline agent tools

test(integration): add case-collision fixture

docs: document end-to-end workflow
```

After each completed logical step:

1. inspect diff
2. run formatter
3. run relevant tests
4. run linter
5. ensure repository still works
6. stage only relevant files
7. create a descriptive commit

Always inspect:

```bash
git diff
git status
```

before committing.

---

# 46. COMMIT QUALITY

Never use meaningless messages such as:

```text
update
fix
stuff
changes
wip
progress
final
```

Use descriptive conventional-style messages.

Do not mix unrelated concerns in one commit.

Every commit should represent a coherent working change.

Do not knowingly commit broken code solely to create more commits.

If a later step finds a previous bug:

```text
create a NEW corrective commit
```

Do not rewrite old commits merely to make history look perfect.

History should reflect real incremental engineering.

---

# 47. PUSHING

If authentication and repository permissions are available, push stable commits regularly.

At minimum push after major milestones.

Do not force-push.

Do not delete remote history.

If pushing fails because of authentication or network restrictions:

- continue implementing
- continue committing locally
- report exactly what could not be pushed

Do not stop the project because push access is unavailable.

---

# 48. DEVELOPMENT MILESTONES

Use incremental vertical milestones.

## Milestone 1 — Foundation

Implement:

- stack selection
- package/project structure
- logging
- errors
- configuration
- CLI skeleton
- formatter
- linter
- CI
- initial tests

Commit logical steps separately.

---

## Milestone 2 — PostgreSQL schema inspection

Implement:

- connection
- schema discovery
- tables
- columns
- PKs
- FKs
- UNIQUE
- NOT NULL
- types
- enums
- checks where feasible

Provide:

```bash
faultline inspect
```

---

## Milestone 3 — Migration runner

Implement:

- raw SQL
- command runner
- timeout
- structured result
- PostgreSQL error capture
- migration fingerprints

---

## Milestone 4 — Isolated experiment lifecycle

Implement:

```text
create isolated DB
restore baseline schema
seed candidate
run migration
observe
destroy/reset
```

Prove isolation works reliably.

---

## Milestone 5 — Schema-valid generator

Implement primitive generators and relationship-aware row generation.

Generated state must satisfy pre-migration constraints.

---

## Milestone 6 — First counterexample strategy

Implement a high-value strategy such as:

```text
collision generation
```

Demonstrate discovery of the case-normalization UNIQUE bug.

This must be real.

---

## Milestone 7 — Search engine

Implement:

- seeds
- experiments
- state fingerprints
- budgets
- strategy scheduling
- result persistence
- resume

---

## Milestone 8 — Minimization

Implement:

- row removal
- table-level grouping
- FK-aware removal
- value shrinking
- ddmin-like reduction

Counterexamples should become small and understandable.

---

## Milestone 9 — Semantic checks

Implement at least one successful-migration failure mode such as:

```text
precision loss
```

Faultline must demonstrate that:

```text
migration success != migration correctness
```

---

## Milestone 10 — Round-trip testing

Implement:

```text
UP
DOWN
COMPARE
```

and discover a real irreversible migration fixture.

---

## Milestone 11 — Reports and replay

Implement:

```bash
faultline report
faultline replay
```

Reports should include real evidence.

---

## Milestone 12 — CI UX

Implement:

- reliable exit codes
- JSON output
- bounded search
- deterministic seeds

---

## Milestone 13 — MCP / AI integration

Only after deterministic core works.

Expose safe structured operations.

Do not allow AI integration to bypass safety.

---

## Milestone 14 — Documentation and release readiness

Produce:

- README
- architecture documentation
- getting started
- config reference
- counterexample concepts
- strategy docs
- CI docs
- AI/MCP docs
- safety docs
- contribution guide
- changelog
- release notes

---

# 49. README QUALITY

The README should immediately communicate the problem.

Suggested opening:

```text
# Faultline

Generate the data your migration wasn't ready for.

Most migration tests verify migrations against data developers already
thought to include.

Faultline does the opposite.

It generates schema-valid adversarial database states, executes the real
migration, observes failures or information loss, and reduces discovered
problems into minimal reproducible counterexamples.
```

Immediately show an example.

Do not bury the core idea below installation instructions.

---

# 50. POSITIONING

Faultline should be positioned as:

> A schema-aware migration counterexample synthesizer.

Or:

> A property-based testing and counterexample reduction engine for database migrations.

Do not position it merely as:

```text
migration linter
AI migration reviewer
SQL static analyzer
database fuzzer
```

Its unique value is the complete loop:

```text
GENERATE
EXECUTE
OBSERVE
MINIMIZE
REPLAY
```

---

# 51. FAILURE EVIDENCE

Every discovered counterexample must be backed by evidence.

Example:

```text
Counterexample #17

Migration:
041_normalize_email.sql

Seed:
912831

Before:
2 rows

Experiment:
migration executed

Observed:
PostgreSQL SQLSTATE 23505

Classification:
unique violation

Minimization:
2,391 rows -> 2 rows

Replay:
100 / 100 reproduced
```

Do not show only model-generated explanations.

---

# 52. REPRODUCTION CONFIDENCE

Transient failures may occur.

Support configurable replay counts.

Example:

```bash
faultline replay counterexample-17 --repeat 100
```

Report:

```text
100 / 100 reproduced
```

If reproduction is unstable, clearly classify the failure as unstable.

Do not silently treat flaky results as deterministic.

---

# 53. NO FAKE COMPLETION

Do not report features as complete when only interfaces exist.

Do not say:

```text
Counterexample minimization complete
```

if reduction has not been demonstrated against a real failing migration.

Do not say:

```text
MCP integration complete
```

unless an actual tool server works.

Do not simulate migration outputs.

The test database must genuinely execute migrations.

---

# 54. CONTINUOUS VALIDATION

Throughout development regularly run:

```text
format
lint
unit tests
integration tests
build
```

Before completing any milestone, validate it.

Before final completion, run the complete available suite from a clean repository state.

---

# 55. FINAL ACCEPTANCE TEST

Faultline is not complete until the following scenario works end-to-end.

## Scenario A — Migration crash

1. Start provided PostgreSQL fixture.
2. Load pre-migration schema.
3. Faultline inspects schema.
4. Faultline generates schema-valid states.
5. Faultline discovers the case-collision dataset.
6. Real migration fails with UNIQUE violation.
7. Faultline records the experiment.
8. Faultline minimizes the dataset.
9. Final counterexample contains only necessary rows.
10. Replay reproduces the failure.

---

## Scenario B — Semantic loss

1. Load a migration that succeeds.
2. Faultline generates a precision-sensitive value.
3. Migration completes.
4. Faultline detects information loss.
5. Faultline minimizes the dataset.
6. Replay reproduces the semantic failure.

---

## Scenario C — Round-trip failure

1. Generate valid V1 state.
2. Run UP.
3. Run DOWN.
4. Detect non-equivalence.
5. Minimize state.
6. Replay reproduces irreversibility.

All three scenarios should be represented by real integration fixtures before declaring the first release complete.

---

# 56. DEFINITION OF DONE

The first serious release should provide a coherent workflow such as:

```bash
faultline init

faultline doctor

faultline inspect

faultline test

faultline status

faultline counterexamples

faultline minimize

faultline replay

faultline report
```

At minimum:

- PostgreSQL integration is real
- migrations execute for real
- state generation is real
- pre-migration constraints are respected
- failures are discovered experimentally
- counterexamples are minimized
- experiments persist
- sessions can resume
- deterministic seeds work
- replay works
- semantic-loss testing works
- round-trip testing works
- safety protections exist
- integration fixtures prove the system
- tests pass
- Git history is clean and incremental
- documentation explains the workflow
- AI remains optional

---

# 57. FINAL REPORT

When implementation is complete, provide a concise but concrete engineering report.

Include:

- implementation summary
- chosen technology stack
- architecture
- PostgreSQL capabilities
- supported migration execution modes
- generation strategies
- minimization algorithm
- supported failure classes
- isolation and safety model
- AI/MCP support
- unit/integration test results
- acceptance-test results
- known limitations
- recommended future work
- exact commands to run demos
- Git commits created
- which commits were pushed successfully
- any remaining local-only commits

Do not provide a vague marketing summary.

Report facts.

---

# 58. FUTURE EXTENSION PATH

The architecture should make future work possible without forcing it into v0.1.

Potential future capabilities:

```text
MySQL support
SQLite support
migration framework adapters
rolling-deployment compatibility
old/new application testing
workload-aware migration testing
lock/deadlock scenarios
performance regressions
multi-step migration sequences
production schema anonymization
custom domain generators
AI-generated semantic hypotheses
distributed experiment workers
web dashboard
CI annotations
GitHub PR integration
```

Do not implement all of these immediately.

Build foundations that can support them.

---

# 59. AGENT BEHAVIOR

Act as a senior engineer entrusted with completing the project.

Do not repeatedly ask:

```text
Should I continue?
What should I implement next?
Would you like me to...
```

Inspect the repository and make sound engineering decisions.

When ambiguity exists:

1. inspect existing code
2. choose the most maintainable option
3. record important decisions
4. continue

Only stop when required information is genuinely impossible to infer and implementation cannot proceed safely.

Do not use minor ambiguity as an excuse to stop.

---

# 60. FINAL DIRECTIVE

Build Faultline as a serious open-source developer tool.

Do not build a portfolio mockup.

Do not build a thin LLM wrapper.

Do not build another migration linter.

Build an experimental system that discovers migration failures developers did not know to test.

Prioritize:

```text
correctness
real PostgreSQL execution
safe isolation
schema-valid generation
counterexample discovery
minimal reproduction
determinism
replayability
clean architecture
testing
developer experience
incremental Git history
```

The central product promise is:

> Faultline generates the data your migration wasn't ready for.

The central technical loop is:

> Generate. Execute. Observe. Minimize. Replay.

The relationship with AI remains:

> AI proposes. Faultline proves.

Begin by inspecting:

https://github.com/beratbesli/Faultline

and its existing Git history.

Then begin implementing immediately.

Do not stop after writing a plan.

Do not leave the project as scaffolding.

Build the working system.

Commit every completed logical step.

Test every claim.

Push stable milestones when permitted.

Finish with a real, experimentally verified Faultline.
