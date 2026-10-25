# `core/assistant-narration.ts`

Authority: complete 52-line module at the [common pin](../source-lock.json).
Rust: `crates/pi-coding-agent-data/src/core/assistant_narration.rs`. [Contract](../../specs/assistant-narration.md).

| Export / branch | Native representation | Proof |
|---|---|---|
| `getAssistantContentText`: text, public thinking, other thinking/tool call | `get_assistant_content_text(&AssistantContent) -> Option<JsString>`; owned unchanged prose | real print-mode differential: narration-only selection, mixed private/public/text/empty-text/end_turn, unpaired human text and whitespace through actual stdout |
| `isServerNarrationSignature`: absent/empty, malformed, public marker | optional scalar signature, shared `buffer_from_base64`, nested borrowed byte fields | actual frozen component signature corpus; hidden/private versus public visible output |
| Base64 standard/URL alphabets, ignored characters, first padding, scalar Unicode low bytes | existing shared Node Buffer decoder, no per-consumer decoder | corpus includes noise, URL forms, Unicode U+0145 mapping, supplementary-character padding behavior |
| protobuf varint shifts 0..28; continuation/truncation | bounded `u64` reader returning absence for the frozen caught error | field corpus includes truncated/five-/six-byte data and nested key cases |
| first matching wire2 field; wire0/1/5 skip; unsupported wire and excessive length | `read_protobuf_field` | duplicate public/private envelope order, zero fields, skip-prefix cases, unsupported group, out-of-range lengths, mutations of every byte of a real narration envelope |
| JS `key.value >>> 3` / `& 7` | explicit u32 narrowing before field/wire extraction | nested keys with bits32/33/34 set retain source classification |
| UTF-8 marker equality | direct equality to ASCII bytes `narration` | private, NUL suffix and BOM-prefixed kind cases; these cannot decode to the exact ASCII marker |

## Consumer integration

- Assistant component maps narration to ordinary text slots, independent of hide-thinking.
- Interactive mode's actual message-start → ignored private/out-of-range thinking-end → public thinking-end sequence refreshes the hidden assistant only when signed narration arrives.
- Print mode's printable predicate and final prose loop call the shared getter. The targeted process fixture uses real frozen services/session/runtime and faux stream; human U+D800 reaches stdout as Node U+FFFD only at that IO boundary. Private thinking is absent; text ordering, whitespace and empty text lines match.

Native print mode has unrelated old selection/end_turn behavior still awaiting its own whole-module re-pin. This bounded helper integration does not mark that row done. The shared mode oracle's removed extension-runtime/ModelRuntime imports were replaced with current loader and public ModelRegistry APIs; no old-path fallback or frozen-source patch is used. Fixture response data now uses the same lossless JSON authority as native content.

## Frozen discrepancy classification

The manual source reader accepts five-byte keys larger than u32 and then narrows them with JS bitwise operators. Therefore invalid protobuf keys whose low 32 bits match a requested field can identify narration. Classified as retained frozen malformed-key behavior, not a new strict-protobuf interpretation or semantic deviation. No protobuf library substitutes a stricter parser.

## Proof ledger

Artifacts `/tmp/pi-rust-repin-lossless-interactive/`:

- `test-assistant-message-red-1.log` → `test-assistant-message-green-1.log`: 4 component differentials, including the signature corpus and real Markdown-cache output.
- `test-assistant-narration-mode-red-1.log` → `test-assistant-narration-mode-green-1.log`: actual mode event-order boundary 1/1.
- `test-assistant-narration-print-red-3.log` → `test-assistant-narration-print-green-1.log`: actual print-mode process 1/1, two response scenarios. Earlier print RED1/2 were stale oracle-import setup errors, not production parity evidence.

Signatures are `Option<&str>` under the explicit common-pin opaque-scalar approval; this is not permission to replace unmatched units in human prose. Returned prose is copied without mutation. Final parent gates remain separate.
