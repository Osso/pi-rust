# Q9 old-only code retirement

Frozen `packages/agent-core/src/index.ts:1–44` exports no tools, telemetry, event bus, reducer or search. Frozen source tree has no corresponding implementations.

| Removed core code | Disposition / proof |
|---|---|
| `harness/tools/{bash,read,edit,write,file_mutation_queue,image,path_utils,tool_context,mod}.rs` | Removed obsolete harness-specific tools and their old-only boundary tests. Coding-agent retains its frozen native tools. |
| `harness/tools/edit_diff.rs`, `jsdiff.rs`, `unicode.rs` | Rehomed to `pi-coding-agent/src/core/tools/{edit_primitives,jsdiff,unicode}.rs`; existing coding-agent edit-diff/path/diff-rendering callers use these local modules. Exact pinned ICU normalizer dependency moves with them. Six focused test modules plus four raw edit differentials rehomed, not weakened. |
| `harness/{events,reducer,telemetry}.rs` | Removed old-only APIs, exports and tests. The new frozen harness hook/event API is a different contract; this deletion is not proof of its implementation. |
| `search/` | Removed old-only search API and tests. Frozen core exports no search. |
| Old scaffold/lane-session/conformance/tool differential targets | Removed: oracle targets referenced deleted TS modules and tested old-only lane records, v4 codec and scaffold rejections. They do not prove the new v3 tree/session/harness modules, which remain unported at this checkpoint. |

Retained utility differential removes only fixtures for retired tools; still runs core truncation, sanitization, resource, prompt/skill/message/value fixtures. Its imports use the frozen core root and coding-agent helpers, not nonexistent `packages/agent` paths. It currently detects a real `truncateHead` terminal-empty-line mismatch, recorded as RED in `tools-core-green.log`; not hidden by test deletion.

Frozen `proxy.ts:345–352` permits only stop/length/toolUse done reasons. Core no longer references `StopReason::Deferred`, `DoneReason::Deferred` or `AssistantMessage.deferred`. While pi-ai still has the old enum member, the proxy match rejects unsupported reasons with an unreachable assertion; remove that last arm when the pi-ai variant is removed. Session context no longer filters assistant messages on that old stop reason; updated test explicitly retains the assistant.

Proof: `cargo test -j4 -p pi-coding-agent --test agent_core_edit_diff_full --test agent_core_lossless_edit --test agent_core_differential_unicode_normalize --test edit_diff`: 27 GREEN, including frozen raw-unit differential cases. `/tmp/pi-rust-repin-delta-agent-core/rehome-green.log`. All-target checks recorded separately in the ledger. No complete harness/session parity claim.
