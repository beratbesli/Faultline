# Security policy

## Supported versions

Faultline is pre-1.0 software. Security fixes target the latest `main`
revision and are included in the next release.

## Reporting a vulnerability

Do not open a public issue. Use GitHub's private **Security advisories → Report
a vulnerability** flow for this repository. If it is unavailable, contact the
maintainer through the private contact method on their GitHub profile. Include
the affected revision, impact, reproduction steps, and suggested mitigation.
Never send real credentials or production records.

## Threat model

Faultline runs migration commands, connects to PostgreSQL, writes generated
data, and persists replayable counterexamples. Configuration, migration files,
database contents, replay bundles, and MCP requests are therefore untrusted
inputs. An operator must assume that:

- a malicious migration or project configuration can execute commands with the
  operator's permissions;
- an incorrect connection string can target valuable data;
- generated reports and replay bundles may contain sensitive schema or data;
- a network bridge around the local MCP stdio interface creates an additional
  authentication and transport-security boundary.

## Safe operation

1. Use a disposable container, VM, or isolated database account.
2. Grant only the privileges required to create and mutate the test database;
   credentials must not reach production.
3. Review configuration and migration commands before execution.
4. Confirm the target identity and schema fingerprint before destructive work.
5. Protect logs, counterexamples, reports, and exported bundles as sensitive
   artifacts and expire them deliberately.
6. Keep MCP on local stdio unless a separately authenticated, encrypted bridge
   is implemented and reviewed.

Faultline does not provide command sandboxing, database authorization,
encryption at rest, or proof that a migration is safe. The detailed operational
guardrails are in [docs/safety.md](docs/safety.md).
