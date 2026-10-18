# Traceability: `packages/session-backends/sqlite-node/src/sqlite/storage/facts.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/storage/facts.rs`. Tests: `facts_query.rs`,
conformance, repository, differential.

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `FactRow` | `FactRow` | facts-query | |
| `appendFact` | `append_fact` | facts-query, conformance | |
| `readLatestFact` | `read_latest_fact` | facts-query, conformance | `key IS NULL` / `key IS ?` keeps SQLite's `IS` semantics. |
| `readLatestLabelFacts` | `read_latest_label_facts` | facts-query, conformance | Returns `(Option<key>, value)`; latest non-null per key. |
| `readFactRows` | `read_fact_rows` | log-query, differential | |
| `deleteFactRows` | `delete_fact_rows` | repository | |
