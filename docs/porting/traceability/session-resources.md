# session-resources traceability

Pinned source: `packages/ai/src/session-resources.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `SessionResourceCleanup` | Callback receiving an optional session id. | `SessionResourceCleanup` returns `Result<(), Box<dyn Error + Send + Sync>>`; throwing maps to `Err`. |
| `registerSessionResourceCleanup(cleanup)` | Adds to a module-global set and returns an unregister function. | `register_session_resource_cleanup` returns a `SessionResourceCleanupRegistration` with `unregister`; `invokes_registered_cleanups_with_the_session_id`, `unregister_stops_invocations`. |
| `cleanupSessionResources(sessionId?)` | Calls every registered cleanup with the id; collects thrown errors; throws `AggregateError(errors, "Failed to cleanup session resources")` when any failed. | `cleanup_session_resources` snapshots the registry, collects `Err` values, and returns `SessionResourceCleanupError { errors }`; `passes_a_missing_session_id_as_none`, `aggregates_cleanup_errors`. |

## Exclusions and deviations

- The registry is a `Mutex<Vec<_>>` snapshot taken before invocation; TypeScript iterates the live `Set`, so a cleanup that registers another cleanup during iteration is not observed. Registration during cleanup is not a supported workflow.
- `AggregateError` is represented by a typed error with an ordered `errors` vector and the same display message.
- Callbacks return `Result` instead of throwing; non-`Error` throw values are not representable.
- No differential oracle: the module owns process-global mutable state and takes arbitrary JS callbacks. Behavior is covered by isolated marker-based tests.
- No source bug found.
