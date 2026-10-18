# Traceability: `packages/agent/src/harness/session/context.ts`

Rust: `crates/pi-agent-core/src/harness/session/context.rs`. Tests: `session_context.rs`, `differential_session.rs`.

## Exports and branches

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `SessionContext` | `SessionContext` | context, differential | Messages, thinking level, model, active tools. |
| `ContextEntryTransform` | `ContextEntryTransform` | context, differential | `Arc<dyn Fn(&[Entry]) -> Vec<Entry>>`. |
| `CustomEntryContextMessageProjector` | `CustomEntryContextMessageProjector` | context, differential | `Arc<dyn Fn(&Entry, usize, &[Entry]) -> Option<Vec<AgentMessage>>>`. |
| `SessionContextBuildOptions` | `SessionContextBuildOptions` | context, differential | |
| `deriveSessionContextState` (internal) | `derive_session_context_state` | context, differential | Default `thinkingLevel: "off"`; assistant messages update the model. |
| `defaultContextEntryTransform` | `default_context_entry_transform` | context, differential | Latest compaction plus following entries; clone-all when absent. |
| `buildContextEntries` | `build_context_entries` | context, differential | Default transform then caller transforms in order. |
| `sessionEntryToContextMessages` | `session_entry_to_context_messages` | context, differential | Message (deferred assistant omitted), compaction summary + retained tail, non-empty branch summary, custom projector. |
| `buildSessionContext` | `build_session_context` | context, differential | |

## Message constructors

`createBranchSummaryMessage` and `createCompactionSummaryMessage` are added to
`crates/pi-agent-core/src/harness/messages.rs` as `create_branch_summary_message` and
`create_compaction_summary_message`. They return `AgentMessage::Custom` objects with TypeScript field order and
JS-number formatting. `CompactionSummaryOptions` covers the optional source fields.

## Gaps

- TS `timestamp: string | number` accepts ISO strings; the typed Rust constructors take `i64` Unix milliseconds.
  Date parsing is an explicit typed-boundary exclusion.
