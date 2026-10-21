# Bounded session storage integration evidence

Target: [common source lock](../source-lock.json), frozen `4f9e6f8a4` read-only. Audited integration: `5989c9a20bc6cea40f9adf70559a8e9132d7962f`, containing storage head `a0eb3b888bbd796f39934c39de77b56cbfcfb9f1`. Evidence is inherited, not a fresh integration gate. Independent verifier49 remains pending at `/tmp/pi-rust-repin-session-storage-verify`.

Provenance: `/tmp/pi-rust-repin-session-storage/REPORT.md` and `ledger.md`. Commands below ran in the storage worktree with its configured test runner, not in the docs worktree.

| Exact scope | Recorded command / revision | Evidence and limits |
|---|---|---|
| Transcript, archive, history, manager, raw migration and listing boundaries | `cargo test -j4 -p pi-coding-agent --test repin_session_storage -- --nocapture --skip storage_worker_hard90second` at `1785172c` | 30 PASS, one filtered; includes frame/UTF-8 errors, rollback/modes/plain precedence, scoped-empty metadata, resident predicate, legacy/raw HTML and tool migration. Not whole-module acceptance. |
| First-message selector | `cargo test -j4 -p pi-coding-agent --test differential_session_search -- --nocapture` at `f0675811` | PASS; actual pinned source excludes subsequent-message text. Later storage changes do not touch this proof scope. |
| Packaged native worker deadline | `cargo test -j4 -p pi-coding-agent --test repin_session_storage storage_worker_hard90second -- --nocapture` at `7b28b70b` | PASS in 90.10s: real FIFO-blocked worker killed/reaped, temporary state removed. Same-branch prior proof; worker boundary unchanged, not rerun. Does not prove release/SDK layout. |

## Current capability boundary

- Receipt raw values and unknown summary keys/values retain escaped UTF-16 through active parsing, retained rewrite, fork and legacy HTML export. Public generic serde wrappers still use checked `Value`/`Map`; no lossless guarantee for those wrappers or unrelated typed content. Active tool migration uses the lossless parser; [tool-output trace](session-tool-output.md) owns its exact proof.
- `SessionInfo` contains `first_message`, not source-absent `all_messages_text`; selector searches id/name/first message/cwd. Populated metadata with an empty scoped result suppresses filesystem discovery/cache; only absent/empty index permits that path. [Manager trace](session-manager.md) owns consumer details.
- Resident exclusion uses literal supervisor/architect IDs or complete slash/backslash path segments, not substring matching. [Predicate trace](resident-session.md) owns the nine-case proof; lifecycle remains open.

## Open boundaries — no new waiver or requirement

| Boundary | Status |
|---|---|
| Public truncated-zstd stream | Frozen source emits unhandled `ZstdDecompress` `Z_BUF_ERROR` and exits; classification unresolved. Rust IO error is not an approved semantic deviation. |
| Resident preparation/live cleanup; archive CLI, idle, deletion and mutable restore callers | OPEN; storage and predicate proofs do not prove these consumers. |
| Durable mailbox acceptance | OPEN: serialized markers, append, persist and fsync do not prove runtime claim/CAS/duplicate-suppression wiring. |
| Packaged SDK/release worker layout | OPEN; private package test layout is not distribution proof. |
| Whole-module/final gate | OPEN; no renewed whole-module completion, broad checks, clippy, readability or final acceptance evidence here. |

Compatibility additions are provenance only, not blanket approval of these open boundaries. Existing [compatibility spec](../../specs/compatibility.md) remains outside this audit's edit ownership.
