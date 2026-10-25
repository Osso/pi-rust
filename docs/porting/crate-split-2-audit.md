# Round-2 crate-split content audit

Compared with merged parent `4c28cdf4`; code snapshot `a87f64cc`. [Graph and proof ledger](crate-split-2.md).

## Pure moves

| Commit | R100 renames |
|---|---:|
| `ff0bf920` | 163 |
| `9e8b92a5` | 4 |
| `ff554589` | 18 |
| `155370c6` | 1 |
| `f4aa710d` | 9 |
| `666931bf` | 4 |

These commits contain **199 R100 renames** and no content edits. Some files move through more than one layer. There are **168 distinct original moved files**: **160 byte-identical** to the merged parent and eight with the edits below. Git detects seven modified renames; the four-line permission module becomes a delete/add pair because its declaration list becomes re-exports.

## Moved-file content edits

Paths are relative to the named crate's `src/` directory.

| Crate | File | Complete content change |
|---|---|---|
| data | `core/trust_manager.rs` | Expose `TrustError::new` for tools' project-trust adapter. |
| storage | `core/host_relay_client.rs` | Expose `random_fraction` for the relocated host-relay resident. |
| storage | `core/session_cwd.rs` | Expose `stat_cwd_directory` for the runtime. |
| storage | `core/session_manager.rs` | Expose `SessionManagerError::native_error` for the runtime. |
| tool-support | `core/tools/abortable.rs` | Expose `observe_tool_producer` for the built-in tools; formatter joins its signature. |
| tools | `core/extensions/types.rs` | Re-export unchanged `HandlerError` from tool-support; move its unchanged `AgentSessionError` conversion to core beside that input type. |
| tools | `core/tools/mod.rs` | Replace moved helper declarations with re-exports; keep the previously private `abortable` alias private. |
| tools | `core/permissions/mod.rs` | Re-export data-owned policy/presets; keep the orchestration declarations here. |

All visibility changes above have captured compiler errors proving the cross-crate use. No function body changes its control flow or error handling.

## Other boundary edits

`core/agent_session.rs` stays in core. It re-exports the unchanged `CycleDirection` and `ModelCycleResult` definitions from tools and imports their unchanged thinking-level serializer. The serializer is public only at its new owner; its core import stays private. The unchanged `AgentSessionError` → `HandlerError` conversion moves here to satisfy Rust's orphan rule. Other callback conversions move unchanged from the old core file into the packages, storage and tool-support owners.

New library/module roots and Cargo manifests declare the layers and compatibility re-exports. Core still owns session runtime/services, SDK, provider recovery, extension actions and footer. Old public `pi_coding_agent_core::core::*` and facade paths remain available. Internal data deliberately does not re-export models/settings, which would restore the dependencies that the split removes.

The only facade test edit is the physical retention-contract include path, now storage-owned. Test assertions and imports do not change. New package test-support symlinks target the existing facade directory; automatic integration/example/bench discovery stays disabled. Existing oracle helpers are included only by packages/tools that actually consume them; no warning suppression is added. Unit-test include depths and asset paths remain valid after the moves.

Rust-path documentation edits are mechanical, including grouped and directory paths. Locked source counts do not change. Parent lane edits are excluded from this audit by comparison with the merged parent.

## Dependency and readability audit

`Cargo.lock` adds seven local packages: data, models, settings, storage, packages, tool-support and tools. External package records, versions and features are unchanged. Workspace profiles/lints and `.cargo/config.toml` are unchanged. No callback/trait inversion, runtime fallback, external dependency, push or deployment is part of this work.

The changed Rust lines add no control-flow complexity, error suppression, fallback or new runtime abstraction. The moved bodies remain unchanged. The remaining edits are declarations, imports, compiler-required visibility, test paths and relocated conversions/types.

Artifacts on the development host: `/tmp/pi-crate-split-2-proof/`; pure-move logs, `move-map.json`, `final-audit-name-status.log`, content patches, compiler boundary errors and `clippy-signature-audit.json`.
