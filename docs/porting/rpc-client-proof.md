# RPC client/types common-pin proof

Verified:2026-10-24. Frozen [source lock](source-lock.json), `rpc-client.ts`640lines / `rpc-types.ts`283lines. [Client map](traceability/rpc-client.md), [type map](traceability/rpc-types.md), [contract](../specs/rpc-client.md). This is a whole client/type pair checkpoint, not whole interactive-mode or legacy JSONL serializer acceptance.

## Capability matrix

| Surface | Implemented/proved | Missing/blocked |
|---|---|---|
| Declared protocol | All32 commands,12 state fields,9 extension UI methods,3 UI answer branches; true cancellation literal; raw UTF-16 and number/count contracts; old-only available-thinking-level API removed | No missing declared branch; typed-core invalid JS runtime input exclusion retained |
| Public client methods | Every command body/getter; declared ModelReference/ModelInfo plus ordered raw extras; success truthiness/local Error/TypeError; raw undefined-aware transport; actual frozen server getters without dropped fields | Foreign malformed/nonrepresentable returns explicitly reject under approved per-getter boundaries, not silent projection |
| Lifecycle/process I/O | Launch/validation/error precedence, executable/argv/cwd/env, failed-spawn identity, stop coalescing and signal escalation, once-error observer, FIFO stdin/EPIPE, exact native errors and stderr descriptor handling | Accepted cold/cancelled public future, envp order, warnings, injected-fault and unreachable private-counter boundaries listed in client map |
| Events/internal promises | Raw envelopes/unknown values; first agent_end including willRetry; live listener removal/caught throw; independent promptAndWait collection timeout and prompt-failure lifetime | No missing internal promise branch; public explicit cancellation retains the approved native boundary |
| Shared integration | JSONL callback/read uncaught errors; detach preserves/drains live child pipe; SIGHUP blocked512KiB flush; complete RPC mode command/UI/signal target | Historical JSONL serializer/decoded-string exclusions are not revalidated by this slice |

## Behavioral proof

All Cargo runtime commands use `test -j4 -p pi-coding-agent` and the configured isolation guard. Artifacts and exact command/revision/invalidation ledger live in `/tmp/pi-rust-repin-lossless-interactive/` (`rpc-client-proof-ledger.md`). No live provider/state, global serialization, ignored case or relaxed assertion.

| Command suffix | Relevant code / proof artifact | Result |
|---|---|---|
| `--lib modes::rpc -- --nocapture` | After readability extractiond93516d3 and parent merge7888ab69 (`e40dff0d`); `rpc-pair-lib-integrated.stdout` |24/24:11 client,4 DTO,9 mode |
| `--test differential_rpc_client -- --nocapture` | Same post-parent source; `rpc-pair-runtime-integrated.stdout` |16/16 |
| `--test rpc_client -- --nocapture` | Same; `rpc-pair-remaining-runtime-integrated.log` |18/18 |
| `--test rpc_client_frozen_server -- --nocapture` | Same; preceding artifact; actual private frozen server with independent destructive scenarios |2/2, every declared getter plus raw export-path falsifier |
| `--test rpc_jsonl_errors --test rpc_jsonl_reader --test rpc_types -- --nocapture` | Same; preceding artifact; later collector alias is type-only |2+5+15/22 |
| `--test differential_rpc_mode -- --nocapture` | ec66cf7e; `rpc-mode-final-calibrated-parallel.log` |30/30, default parallel target |

Composite current behavioral scope:112 passing cases. The100ms thinking-timeout fixture passed in isolation but raced its own recovery under the parallel matrix. Parent approved1000ms on both runtimes; pending-first timeout, successful retry, exact event/message and persisted-abort assertions remain unchanged. Full parallel confirmation passes20.69s. No production timer/scheduler change.

## Distinct RED→GREEN evidence

The ledger preserves actual-source/real-private-process REDs for raw correlation/first-end, partial declared models, Error truthiness, option UTF-16 and validation precedence, asynchronous spawn lifetime, live listeners, stop coalescing, non-reading2MiB stdin, closed stdin, live stdout detach, typed raw entries/leaves/export paths, independent collection-promise lifetime and signal-only startup. Shared fixes include all31 Node Linux signal numbers and canonical aliases; original read/callback errors; and fatal diagnostic sink failure (actual Node exit1 vs former native abort134).

Descriptor probes compare real EPIPE socket and /dev/full filesystem errors, default fatal status, and literal directory/closed-before-exec/datagram dummy sinks while retaining captured child stderr. Private crash probes alone lower their core limit; host stderr/limits are untouched.

## Static/format proof

| Gate | Evidence |
|---|---|
| Strict pinned no-emit TypeScript | PASS, `rpc-pair-strict-typescript-es2024.log`; six TS fixtures/source modules plus frozen client/types. Temporary ES2024 target only, citing the single [TS1501 note](oracle-gate.md#frozen-v-target-discrepancy-2026-10-24); no source patch/shared-builder change |
| Biome, six changed oracle files | PASS, `rpc-pair-biome-current.log` |
| Rust workspace all targets |0 errors/0 warnings at7888ab69; `rpc-pair-workspace-integrated.*`; final post-alias confirmation pending |
| Warning-mode workspace all-target/all-feature clippy | Prior command exits0,1680 diagnostics; one changed-line test tuple complexity replaced by an alias. Final changed-hunk confirmation pending; no suppression |
| Readability | Manual changed Rust/TS audit; sequential guards, live dispatch and command configuration named explicitly. Metric executables unavailable, no automated metric claim |
| Format/inventory | Final checkpoint gate pending; frozen source file/line totals must remain686/186679 |

## Integration boundary

Parent e40dff0d is merged. Parent owns final integration and independent verification. The separately announced callback-uncaught relocation will be merged when its checkpoint is supplied; no agent, push, deploy, package/version change or source modification is performed by this lane. Existing rustix1.1.5 adds only its approved net feature for safe descriptor metadata; Cargo.lock was unchanged before/after its focused proof.
