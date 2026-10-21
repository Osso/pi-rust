# Bounded session storage integration evidence

Target: [common source lock](../source-lock.json), frozen `4f9e6f8a4` read-only. Audited integration: `5989c9a20bc6cea40f9adf70559a8e9132d7962f`, containing storage head `a0eb3b888bbd796f39934c39de77b56cbfcfb9f1`. Independent bounded verification at `5989c9a2` passed 30 storage tests, the source search differential and byte serializer. Initial four lint failures were fixed by `0c414850`; exact targeted clippy passed at docs-only successor `e5d6c244`. Full scope, commands and limits: `/tmp/pi-rust-repin-session-storage-verify/REPORT.md`. This is not whole-module acceptance.

Provenance: `/tmp/pi-rust-repin-session-storage/REPORT.md` and `ledger.md`. Inherited commands below ran in the storage worktree with its configured test runner. Fresh verification ran in canonical; the two post-fix history regressions and footer usage regression also passed there after `0c414850` (`post-lint-*.log` beside the independent report). The full 30-test target was not repeated for those mechanical cleanup changes.

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
| Resident preparation/live cleanup; archive CLI, idle, deletion and mutable restore callers | Bundled archive/delete consumers now have [bounded proof](../session-archive-history-proof.md). Resident preparation, idle/archive CLI and mutable restore callers remain OPEN. |
| Durable mailbox acceptance | OPEN: serialized markers, append, persist and fsync do not prove runtime claim/CAS/duplicate-suppression wiring. |
| Packaged SDK/release worker layout | OPEN; private package test layout is not distribution proof. |
| Whole-module/final gate | OPEN; focused tests and targeted clippy passed, but no broad or final acceptance. Manual readability findings remain; numerical metrics unavailable. |

Compatibility additions are provenance only, not blanket approval of these open boundaries. Existing [compatibility spec](../../specs/compatibility.md) remains outside this audit's edit ownership.
