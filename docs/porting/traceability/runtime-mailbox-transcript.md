# runtime-mailbox-transcript.ts

Frozen source: [4f9e6f8a4](../source-lock.json). Rust: `core/runtime_mailbox_transcript.rs`.

| Export / branch | Rust / proof |
|---|---|
| `RUNTIME_MAILBOX_ACCEPTANCE_CUSTOM_TYPE` | Same string; recovery custom-entry fixture. |
| `mailboxStoreRefsEqual` | `(&JsValue,&JsValue) -> Result<bool,ControlDbError>`: JS property access/null errors, strict primitive/reference equality and UTF16 string identity; no eager field validation. |
| `readAcceptedRuntimeMailboxStoreRefs` | `Option<&JsString> -> Result<Vec<pi_js_compat::js_json::JsValue>,ControlDbError>`: lossless JSON strings/keys/numbers, whole transcript, source ordering, complete LF records only, message/custom refs, inactive branches. Actual TS helper fixtures cover raw `[{}]`, null and malformed complete records, ignored valid partial final JSON, lone surrogates and distinct surrogate/U+FFFD keys, all malformed ref array elements and JS string iteration. |
| Undefined/empty/missing file | Empty refs; missing file does not synthesize acceptance. |
| Other read/parse errors | Original native Node read record (constructor/message/code/errno/syscall/path) and lossless SyntaxError are propagated before DB recovery mutation. `transcript_read_errors_match_frozen_node_diagnostics` covers EISDIR/ENOTDIR; `malformed_complete_transcript_retains_frozen_syntax_error` covers malformed complete lines. No eager schema validation of persisted refs. |

## Whole-module audit — storage-3 (2026-10-25)

All three exports and every realistic branch of the 35-line frozen module were compared literally with the native implementation. Existing source-created receipt fixtures cover appendMessage and appendCustomEntry in persisted JSONL, inactive branch receipts, ordering, empty collections, missing path/file and partial final records; reciprocal mailbox acceptance is recorded in [storage successor proof](../storage-successor-proof.md). The six-test `suite_session_1 repin_runtime_mailbox_transcript` subset is the focused gate. The two new tests first failed against baseline2683e5cbc: directory reads flattened Node diagnostics into OS Display text; JSON.parse retained only text instead of SyntaxError. The fix reuses shared Node FS and lossless JSON authorities without new carriers or APIs. RED logs `/tmp/storage3-transcript-{read,syntax}-red.log`. Follow-up `/tmp/storage3-transcript-constructor-red.log` also asserts actual constructor identity (not merely the mutable Error.name); the native parser now constructs NativeErrorClass::SyntaxError explicitly.

Gap: adversarial wrong-typed/exotic persisted JSON and OS/runtime faults are excluded by the current parity bar; existing raw-ref tests remain unchanged. No whole control/store or runtime-delivery acceptance is implied.

Lossless follow-up: actual TS `SessionManager.create/appendMessage/appendCustomEntry/persistForRecovery` emits escaped lone-surrogate receipts. Legacy serde reader RED replaced those units and collapsed distinct keys; noniterable collection error wording also differed. `transcript-lossless-green.log` GREEN12 (mailbox8 / transcript4). Rust lossless wire output equals the same TS process's helper output. Dead-claim recovery proves a lone-surrogate receipt cannot alias a U+FFFD SQLite key and falsely delete a claim. No UTF-8 lossy conversion is used on JSON refs; filesystem byte decoding remains the explicit Node-equivalent boundary.

DB recovery tests use an owned TS oracle process to create and claim canonical rows, kill that fixture owner, then replace registration with Rust. Complete receipt removes dead claim rather than requeue. This is not proof that runtime append/fsync callers persist receipts crash-safely; those caller boundaries remain separate work.
