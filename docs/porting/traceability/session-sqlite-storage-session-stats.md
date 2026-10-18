# Traceability: `packages/session-backends/sqlite-node/src/sqlite/storage/session-stats.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/storage/session_stats.rs`. Tests: conformance,
repository (usage accounting), differential.

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `createStats` | `create_stats` | conformance | |
| `readStats` | `read_stats` | conformance, differential | `message_count` is `u64`, token/cost fields `f64`. |
| `incrementMessageCount` | `increment_message_count` | conformance | `changes != 1` → `Missing stats row for session {id}`. |
| `addUsageToStats` | `add_usage_to_stats` | conformance, repository | cached += cacheRead; uncached += input + cacheWrite; total += totalTokens; cost += cost.total. |
| `deleteStats` | `delete_stats` | repository | |
