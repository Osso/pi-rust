# Ask-secret: current-pin module audit

Source: `coding-agent/extensions/ask-secret/src/index.ts` at [4f9e6f8a4](../source-lock.json).
Native: `crates/pi-coding-agent-extensions/src/bundled_extensions/ask_secret.rs`.

## Exports

| Frozen symbol | Native mapping | Behavioral proof |
|---|---|---|
| default extension / `createAskSecretToolDefinition` | `ask_secret_extension` / `create_ask_secret_tool_definition` | Exact registration/schema and tool results in `bundled_extensions_tools::ask_secret_matches_typescript`; default-process registration also compares |
| `AskSecretBrowserField`, `AskSecretProvisionRequest`, `AskSecretProvisioner` | Same typed records; ordered `Vec<JsString>` values | Frozen/native callback request, prompts and values; real default argv/stdin captures |
| `AskSecretRenameFile`, `AskSecretToolOptions` | Native rename/provision callback seams | Actual rename plus injected failed rename; no legacy selector-interface path |
| `AskSecretDetails` | Optional path/record and status, omitted when absent | Exact serialized success/cancelled/unavailable results |

## Realistic branches

| Frozen branch | Proof |
|---|---|
| Exactly one of file/browser shapes, paired required fields, string/field validation | 14 concrete source/native validation requests in `ask_secret_validation_and_masked_cancellation_match_frozen` |
| TUI requirement, ordered browser fields, password-only masking, cancellation at either field | Exact source/native UI/result/callback traces |
| File write, UTF-8 bytes plus newline, new 0700 parents and 0600 file | Real source/native writes, byte/mode comparison, no remaining temporary paths |
| Existing file replacement | `ask_secret_atomic_overwrite_matches_frozen`: restore original bytes/0644 before native write, compare replacement/0600 |
| Reject symlink/non-regular destination before UI | Source/native refusal and unchanged existing bytes |
| File cancellation/pre-abort | No parent directory or file created; exact result/error and dialog signal options |
| Rename failure | Source/native injected rename error, unchanged original file and no temporary artifacts |
| Default browser process | `ask_secret_process`: private fake authsudo, actual frozen/native default provisioners, exact argv and raw JSON stdin; success, exit 7, running abort and pre-abort |
| Default-process UTF-16 values | Unmatched secret unit survives as exact JSON escape bytes through actual child stdin |
| Running abort | RED found native kill-on-drop SIGKILL; native now sends SIGTERM and waits for closure, checked by the real fake process signal handler |
| Missing executable | Dedicated source-process crash assertion and explicit native error assertion; approved discrepancy below |

The process fixture never invokes a live broker. Missing-executable proof uses
a private PATH containing only node. The other cases shadow authsudo with the
fixture executable. Only child probes carry `#[ignore]`, and their parent tests
invoke them. Frozen source remains unchanged.

## Proof and boundaries

REDs: old registration/schema mismatch; file request rejected for missing record;
default-process abort failed to invoke SIGTERM handler. Expanded persistence/UI
matrices exposed a test recorder omission: it discarded signal/timeout dialog
options. The recorder now preserves present fields, with AbortSignal's actual
JSON representation `{}`; comparisons remain exact.

Logs: `/tmp/triage-ask-secret-{green,matrix,negative-green}.log`,
`/tmp/triage-ask-secret-process-{red2,green,final}.log`. Proof revisions and later
invalidation are tracked in `/tmp/repin-baseline-triage-ledger.md`.

- gap / DISCREPANCY (approved 2026-10-25): frozen crashes on unhandled ChildProcess `error` when authsudo is absent; native reports `Secrets Broker provisioning failed`; both outcomes have explicit real-process assertions.
- gap: Wrong-typed/exotic JSON, close/combined-cleanup OS faults and independent scheduling races follow the project parity exclusions; ordinary validation, persistence and process failures stay explicit.

Owned callback arguments follow the existing native-owned-value approval; JS
mutation-through-retained callback references is not a separate native API.
