# Q18 lossless shared message boundary

Contract: [lossless content](../../specs/lossless-content.md). Frozen source: `4f9e6f8a4`, read-only.

| Capability | Implemented | Missing / blocked | Proof |
|---|---|---|---|
| `TextContent.text` / `ToolResultMessage.details` | `JsString` / `Option<JsValue>`, explicit-null adapter | downstream consumers pending | `pi-ai/lossless_content`: exact surrogate units, nested keys, omission/null, Rust→TS→Rust-visible JSON |
| Shared serde and object fields | raw-fragment writer/parser; object-only shared adapters | non-JSON serde formats unsupported | `pi-js-compat/js_json_serde`: five focused sandbox cases |
| Content extraction and text streaming final/delta fields | lossless `JsString` | user/thinking/custom strings not migrated yet | pi-ai crate compiles; general provider target regressions pending |
| Provider JSON bodies and `OnPayload` | lossless callback type, scalar builder move into AST; pi-messages direct lossless Context serialization | callback fixtures downstream pending | captured actual HTTP request text/details/callback unmatched-unit escapes equal Node `JSON.stringify` |
| Explicit provider sanitation | frozen per-call-site drop behavior | broad provider regressions pending | pinned `sanitize-unicode.ts` compared against Rust on the same message units |
| Node UTF-8 stream/path/process boundary | shared `encode_node_utf8` | consumer migration pending | distinct high/low-unit replacement test; original authority unchanged |
| Agent loop / JSONL / questions / mailbox / session name | pending next checkpoint | no end-to-end claim | not yet proven |

## Development proof ledger

Artifacts under `/tmp/pi-rust-repin-lossless-content/`:

- `serde-red.log`: shared serde API missing (compiler RED); `serde-green.log`: original three cases pass.
- `content-red.log`: original ToolResultMessage rejected a lone surrogate at deserialize (behavioral RED).
- `helpers-red.log`: new explicit boundary/object helper APIs missing (compiler RED).
- `js_json_serde-green.log`: five focused helper/serde cases pass.
- `content-provider-diff.log`: oracle fixture failed because TSX selected CJS and rejected top-level await; fixed fixture, not counted as behavioral RED.
- `content-provider-diff-green.log`: two sandbox differentials pass, including actual pi-messages HTTP body capture and callback replacement.

Checkpoint proof is bounded. It does not establish workspace compilation, unchanged provider test fixtures, all-source parity, or JSONL integration.
