# Configuration Reference (`faultline.yaml`)

Faultline uses a declarative YAML configuration file:

```yaml
version: 1

project:
  name: "my-migration-project"

database:
  type: postgres
  url_env: DATABASE_URL               # Environment variable containing connection string
  url: "postgres://user@localhost:5432/my_db" # Optional direct URL
  allow_non_isolated: false           # Set true only to override safety guards

migration:
  up_sql: "./migrations/041.sql"      # Path to up migration SQL file
  down_sql: "./migrations/041_down.sql" # Optional down migration SQL file
  up:
    command: "./scripts/migrate-up.sh" # Shell command runner alternative
  down:
    command: "./scripts/migrate-down.sh"
  timeout_secs: 30                    # Execution timeout in seconds

testing:
  experiments: 500                    # Maximum candidate states to test
  seed: 912831                        # Deterministic random seed
  time_limit_secs: 300                # Optional time budget
  max_rows_per_table: 100             # Maximum rows generated per table
  strategies:                         # Active strategies (collision, precision, nullability, boundary, random)
    - collision
    - precision

checks:
  migration_success: true             # Catch SQLSTATE execution errors
  roundtrip: false                    # Test UP -> DOWN reversibility
  semantic_loss: true                 # Detect information loss

invariants:
  preserve:
    - users.id
    - users.created_at
  ignore:
    - users.normalized_email
```
