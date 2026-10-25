# claude-memory-enrich re-pin traceability

Source: `packages/coding-agent/extensions/claude-memory-enrich/src/index.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` (299 lines).
Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/claude_memory_enrich.rs`.

| Frozen symbols / branches | Rust / proof |
|---|---|
| constants, `isHookOutput`, `parseAdditionalContext` | Exact 75000ms timeout, 1000ms kill grace; shape/trim/JSON through lossless carrier. `claude_memory_enrich_matches_typescript` tests ok, empty, invalid shape and failed exit. |
| `resolveClaudeMemoryCommand` | Nonempty env then existing default; empty env no longer spawns empty command. Differential fake executable selection. |
| runtime/active-process state, `queueEnrich` | Per-extension FIFO mutex and permanent shutdown signal. Real-child `queues_enrichment_and_shutdown_waits_for_graceful_close` proves serialization and two successive context injections. |
| `requestEnrichTermination`, resource cleanup, settlement, abort registration, close interpretation | SIGTERM then SIGKILL after grace, wait for output/close before queue release; pre-aborted Error reason preserved; running abort text `operation aborted`; timeout text exact. Real-child shutdown proves SIGTERM, delayed close, no injection. Kill escalation and 75s timeout implemented but not separately timed in integration proof. |
| `runEnrich`, output capture | `enrich` argv, inherited env, JSON newline stdin, concurrent pipe capture, error and exit handling; differential scripts and real-child integration. |
| `shutdownEnrichRuntime`, default registration | Shutdown signal blocks new/queued starts and waits on active queue. Oracle asserts exact handler registration; real-child test asserts no post-shutdown child. |
| `appendEnrichment`, before-start guard, stderr | Empty prompt/empty enrichment no-op, section idempotence, exact framing; oracle compares results. `enrichment_json_and_prompt_injection_retain_raw_units` proves raw units. |

## Gaps

- Adversarial wrong-typed/exotic JSON, OS pipe/signal failures and scheduling races follow the approved user-observable parity bar. No new carrier for these cases.
- Error logs use stderr; the existing oracle checks return values, not captured stderr. Frozen literal messages are retained.

The obsolete 15s/SIGKILL-only/Node-spawn-abort gaps are removed. Runtime is native, not a JS promise/object identity port. Proof: [lane ledger](../bundled-ext-b-proof.md).
