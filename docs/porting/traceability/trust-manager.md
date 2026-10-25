# trust-manager traceability

Pinned source: `packages/coding-agent/src/core/trust-manager.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent-data/src/core/trust_manager.rs`.

Differential proof: `oracle/trust-manager-oracle.ts` runs a 12-step sequence against a real
`ProjectTrustStore` in a temporary tree and returns every result plus the final `trust.json` bytes;
the Rust run repeats the sequence in the same tree after resetting the file
(`tests/differential_config_auth_state.rs`). Focused tests: `tests/trust_manager.rs` (6 cases).

Cross-runtime locking: the store acquires `<trust.json>.lock` through `utils::lockfile`, whose
protocol compatibility with the pinned `proper-lockfile` is proven in
[lockfile traceability](lockfile.md) (both directions, live-refresh and stale rules).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `trust-manager.ts:30-32` | Decisions are `true`, `false`, or `null`; `null` deletes the entry | `ProjectTrustDecision`; inheritance focused test + differential |
| `trust-manager.ts:34-51` | Nearest entry walking up from the canonical cwd | `find_nearest_trust_entry`, `normalize_cwd`; inheritance focused test + differential |
| `trust-manager.ts:53-58` | Parent path of the canonical cwd, `undefined` at the root | `get_project_trust_parent_path`; options test + differential |
| `trust-manager.ts:60-99` | Option list: trust, trust parent (clears the child), optional session-only, do not trust | `get_project_trust_options`; `trust_options_describe_each_choice` + differential (both `includeSessionOnly` values) |
| `trust-manager.ts:101-122` | Read errors: parse failure, non-object, non-boolean value | `read_trust_file` with `utils::js_json`; `invalid_trust_files_report_their_specific_errors` + differential |
| `trust-manager.ts:124-136` | Sorted keys, two-space JSON, trailing newline | `write_trust_file` (UTF-16 key order); file-content test + exact byte comparison in the differential |
| `trust-manager.ts:138-175` | Lock acquisition with ten 20 ms attempts on `ELOCKED` | `acquire_trust_lock`; exercised by every store test |
| `trust-manager.ts:177-233` | `hasTrustRequiringProjectResources`: `.pi` resources, `.agents`/`.codex`/`.claude` skills, user-level exclusions | `has_trust_requiring_project_resources`; `trust_requiring_resources_are_detected_from_project_layouts` (child probe for `HOME`) + differential |
| `trust-manager.ts:235-253` | `get`/`getEntry`/`set`/`setMany` under the file lock | `ProjectTrustStore`; focused + differential |

## Gaps

- `hasTrustRequiringProjectResources` reads `HOME` at call time like the source; tests isolate it in
  a child process.

## Discrepancies

- None observed: results and `trust.json` bytes match the pinned module for the full sequence.

## Proof inventory

- Focused cases: 6.
- Differential: 1 sequence (12 steps) plus exact trust-file bytes.

## Q18 native trust continuation (2026-10-22)

Trust persistence already uses native JsObject path keys with lossless JSON.
ProjectTrustContext/Event cwd, resolution callbacks, prompts/options/selections,
and CLI startup UI now use JsString. CLI notification coloring stays native until
stderr write. Project-trust UI selection errors propagate like the frozen source;
no `.ok().flatten()` suppression. Trust-directory error context retains original
units instead of reusing its encoded filesystem argument.

`lossless_project_trust` covers raw trust-key persistence/reopen, raw cwd prompt,
and raw UI failure propagation. Written but unexecuted while coding compilation
is blocked. replacement-cli-2 has zero trust/resource/package module primaries;
no runtime PASS or renewed frozen differential parity claimed.

NodeFS caller follow-up: pass original JsString to node_read_error, not the
encoded filesystem argument. Raw unreadable trust-store path fixture written,
unexecuted while coding compilation remains blocked.
