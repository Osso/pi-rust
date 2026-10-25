# Bounded session storage integration evidence

Current successor checkpoint: [storage proof through65e23c966](../storage-successor-proof.md). The evidence below remains historical unless renewed there; no whole-module acceptance is claimed.

Target: [common source lock](../source-lock.json), frozen `4f9e6f8a4` read-only. Audited integration: `5989c9a20bc6cea40f9adf70559a8e9132d7962f`, containing storage head `a0eb3b888bbd796f39934c39de77b56cbfcfb9f1`. Independent bounded verification at `5989c9a2` passed 30 storage tests, the source search differential and byte serializer. Initial four lint failures were fixed by `0c414850`; exact targeted clippy passed at docs-only successor `e5d6c244`. Full scope, commands and limits: `/tmp/pi-rust-repin-session-storage-verify/REPORT.md`. This is not whole-module acceptance.

Provenance: `/tmp/pi-rust-repin-session-storage/REPORT.md` and `ledger.md`. Inherited commands below ran in the storage worktree with its configured test runner. Fresh verification ran in canonical; the two post-fix history regressions and footer usage regression also passed there after `0c414850` (`post-lint-*.log` beside the independent report). The full 30-test target was not repeated for those mechanical cleanup changes.

| Exact scope | Recorded command / revision | Evidence and limits |
|---|---|---|
| Transcript, archive, history, manager, raw migration and listing boundaries | `cargo test -j4 -p pi-coding-agent --test repin_session_storage -- --nocapture --skip storage_worker_hard90second` at `1785172c` | 30 PASS, one filtered; includes frame/UTF-8 errors, rollback/modes/plain precedence, scoped-empty metadata, resident predicate, legacy/raw HTML and tool migration. Not whole-module acceptance. |
| First-message selector | `cargo test -j4 -p pi-coding-agent --test differential_session_search -- --nocapture` at `f0675811` | PASS; actual pinned source excludes subsequent-message text. Later storage changes do not touch this proof scope. |
| Packaged native worker deadline | `cargo test -j4 -p pi-coding-agent --test repin_session_storage storage_worker_hard90second -- --nocapture` at `7b28b70b` | PASS in 90.10s: real FIFO-blocked worker killed/reaped, temporary state removed. Historical proof invalidated by approved in-process replacement; see [current spool trace](session-transcript-spool.md#in-process-worker-boundary-2026-10-25). |

## Current capability boundary

- Receipt raw values and unknown summary keys/values retain escaped UTF-16 through active parsing, retained rewrite, fork and legacy HTML export. Public generic serde wrappers still use checked `Value`/`Map`; no lossless guarantee for those wrappers or unrelated typed content. Active tool migration uses the lossless parser; [tool-output trace](session-tool-output.md) owns its exact proof.
- `SessionInfo` contains `first_message`, not source-absent `all_messages_text`; selector searches id/name/first message/cwd. Populated metadata with an empty scoped result suppresses filesystem discovery/cache; only absent/empty index permits that path. [Manager trace](session-manager.md) owns consumer details.
- Resident exclusion uses literal supervisor/architect IDs or complete slash/backslash path segments, not substring matching. [Predicate trace](resident-session.md) owns the nine-case proof; lifecycle remains open.

## Open boundaries — no new waiver or requirement

| Boundary | Status |
|---|---|
| Public truncated-zstd stream | Frozen source emits unhandled `ZstdDecompress` `Z_BUF_ERROR` and exits; classification unresolved. Rust IO error is not an approved semantic deviation. |
| Resident preparation/live cleanup; archive CLI, idle, deletion and mutable restore callers | Bundled archive/delete consumers now have [bounded proof](../session-archive-history-proof.md). Resident preparation, idle/archive CLI and mutable restore callers remain OPEN. |
| Durable mailbox acceptance | Reciprocal source/native enqueue/claim/recipient JSONL acceptance and10 runtime mailbox cases pass in the [successor checkpoint](../storage-successor-proof.md). Injected crash/fsync/peer-CAS and whole-delivery acceptance remain OPEN. |
| SDK archive decoding | Package-executable dependency removed; missing-package child differential passes with exact bytes/mode/cleanup. No release worker asset remains. |
| Whole-module/final gate | OPEN; current focused proof and changed-line readability are recorded in the successor checkpoint. Strict lint is blocked by existing diagnostics; numerical metrics are unavailable. No broad or independent final acceptance. |

Compatibility additions are provenance only, not blanket approval of these open boundaries. Existing [compatibility spec](../../specs/compatibility.md) remains outside this audit's edit ownership.
