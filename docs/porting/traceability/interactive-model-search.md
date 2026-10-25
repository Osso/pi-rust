# model-search.ts

Source: `coding-agent/src/modes/interactive/model-search.ts`
Rust: `crates/pi-coding-agent-interactive/src/modes/interactive/model_search.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `ModelSearchItem` | `ModelSearchItem` | differential (`modelSearch`) |
| candidates `[id, provider/id, provider id]` | `get_model_search_candidates` | differential |
| truthy `name` appends the display name | `name.is_empty()` guard | differential (empty name excluded) |

Discrepancy (faithful): the source's `if (name)` drops an empty display name; the first Rust implementation appended it and the differential test caught the mismatch.
