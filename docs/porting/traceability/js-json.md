# js_json traceability (support module)

Rust: `crates/pi-js-compat/src/js_json.rs` (`pi_js_compat::js_json`), the single
implementation shared by `pi-ai`, `pi-agent-core`, and `pi-coding-agent`.
Source contract: the JavaScript engine's own `JSON.parse` / `JSON.stringify` (V8 as shipped by the
pinned Node runtime), required because persistent agent files are shared byte-for-byte with the
TypeScript runtime and their read errors embed V8 messages.

Differential proof: `oracle/js-json-oracle.ts` runs the engine's built-ins over 1617 inputs
(`crates/pi-coding-agent/tests/differential_js_json.rs`): 11 seed documents plus every truncation,
single-character deletion, and insertion of 26 structural characters at every position, plus
whole-source special values. Comparisons are byte-for-byte on `JSON.stringify(value)`,
`JSON.stringify(value, null, 2)`, and the `SyntaxError.message`. Focused tests:
`crates/pi-coding-agent/tests/js_json.rs` (6 cases).

| Observable contract | Rust mapping and proof |
|---|---|
| `JSON.parse` accepts JSON grammar with only space/tab/LF/CR as whitespace | `Parser`; differential mutations cover BOM, NBSP, control characters |
| Duplicate keys keep the first position and the last value | `IndexMap::insert`; `parse_keeps_last_duplicate_value_at_first_position`, lossless differential actual-codeunit duplicates |
| Numbers become IEEE-754 doubles; `-0` prints as `0`; `1e400` becomes `null` on output | `pi_js_compat::number::js_number_value` + `js_number_to_string`; `numbers_round_trip_as_javascript_doubles`, differential seeds |
| Object keys: array-index keys ascending first, then insertion order | `object_entries`; `stringify_orders_array_index_keys_first_like_javascript_objects` |
| `JSON.stringify` escapes `"`, `\`, `\b\f\n\r\t`, other C0 as `\u00xx` | `write_string`; `pretty_output_matches_json_stringify_with_two_space_indent` |
| Pretty output uses two spaces per level and `": "` separators | `stringify_pretty`; same test |
| `SyntaxError` templates: end of input, dedicated messages with `at position N (line L column C)`, `Unexpected token 'x', ...` with 10-unit context | `unexpected_token`, `unexpected_token_with_context`, `error_at`; `parse_errors_use_v8_messages_with_positions` and the differential suite |
| Positions and line/column count UTF-16 units; `\r\n`, `\r`, `\n` each end a line | `utf16_position`, `line_and_column`; differential cases with CR-only and CRLF inputs |
| Whole-source `NaN`/`Infinity`/`undefined`/`[object Object]` report `"x" is not valid JSON` | `SPECIAL_SOURCES`; differential suite |
| Escapes: `\uXXXX`, surrogate pairs, `\/` | `parse_unicode_escape`; `string_escapes_decode_surrogate_pairs` |

## Native typed decode (2026-10-22)

`from_value` now uses `js_json/value_deserializer.rs`, visiting native values rather than stringify→serde_json::from_str. Its native raw-fragment visitor bridge retains JsString units/keys, nested JsValue nonfinite values and -0. f64/Option<f64> see the original numbers; integer visitors validate finiteness, integrality and target bounds. Numeric/boolean Rust map keys retain prior JSON-key parsing (including decimal key precision above 2^53); tuple visitors cannot silently leave extra elements. No dependency or per-field fallback added.

RED: `js-value-native-decode-red-2.log` reproduced nonfinite→null for floats/nested values; `model-infinity-native-red.log` reproduced a source-valid cached autoCompactionThreshold:1e400 becoming absent. `js-value-native-map-keys-red.log` and `js-value-native-tuple-red.log` guard the existing key/tuple contracts during the adapter replacement. Development GREEN logs: `js-value-native-decode-green-4.log` (five tests) and `model-infinity-native-green-2.log` (actual frozen module); checkpoint evidence lives in the lane ledger. Model/ModelProperties now call the shared adapter, not another stringify boundary. Actual frozen startup observation checks that the optional value is a present nonfinite number before comparing its JSON-null output.

### Caller audit

Search artifacts: `/tmp/pi-rust-repin-delta-ai/js-value-from-value-{production-callers,import-aliases,alias-callers}.txt`. Qualified grep found129 occurrences excluding test/example directories (some are source cfg(test) blocks); aliased imports were also followed.

| Consumer group | Collapse dependency / changed behavior |
| --- | --- |
| ModelWire and ModelProperties | Confirmed bug: optional Infinity became None; shared root proof above, no per-field workaround |
| auth/types.rs AuthEvent::DeviceCode | intervalSeconds/expiresInSeconds explicitly decode Option<f64>; same old accidental null collapse, now original numbers reach those fields |
| harness/session/types.rs and jsonl/storage.rs aliases | Typed messages/records may now retain nonfinite floating fields instead of a decode failure filtered by ok(); no source rule requested the old JSON projection |
| Settings, RPC, IPC, provider/auth and tool/manifest decoders | No documented intentional nonfinite→null policy found; f64 fields retain numbers and integer fields reject invalid numbers instead of receiving invented null |
| serde_json::Value-typed destinations | Their own JSON-only representation remains a separate limitation; this change does not claim those consumers are native arbitrary-JSON authorities |

Parser/stringifier and explicit JSON egress behavior are unchanged. Native from_value integer rejection is explicit; no lossy scalar-string fallback is added.

## Callers

Every ported `JSON.parse` site whose error text reaches output, events, or files uses this
module instead of `serde_json::from_str`: `pi-ai` (Mistral SSE, pi-messages SSE, OpenAI SDK
body/SSE, Codex SSE/WebSocket, Google SSE/credentials, Anthropic OAuth token bodies, Bedrock SSO
token cache, JSON-repair rethrow), `pi-agent-core` (proxy SSE), and `pi-coding-agent` (pyrun
runner/manifest/activation/evaluation, codex-usage, bwrap, supervisor project resolver, detached
bash manifests/completions, supervisor request rows, persisted process identity).

## Discrepancies

- Legacy `parse` replaces lone surrogate escapes with U+FFFD before key insertion, preserving existing serde-facing behavior. This is a known lossy API boundary, not valid proof of raw-file validator parity: `JSON.stringify` must escape those units rather than replace them. New `parse_lossless` retains them. Error messages retain raw UTF-16, including V8's single-unit non-BMP token and split excerpt pairs. Only `Display` encodes Node UTF-8, replacing lone units with U+FFFD. The existing legacy mutation oracle applies `toWellFormed()` only to UTF-8 diagnostic errors; the dedicated error-unit oracle compares actual message units without normalization.
- Number formatting uses the shared ECMAScript primitive; same-bit compact/pretty, coercion, serde writer and parse/write proof: [number trace](js-compat.md#number--parseint--parsefloat--numbertostring-srcnumberrs). Generic serde storage remains numeric-semantic, not byte-exact JavaScript text; [boundary contract](../../specs/js-number-format.md).
- The shared writer converts serde numbers to binary64, matching JavaScript rounding
  above 2^53. Lossless parsing now retains overflow infinities and signed zero until
  serialization; legacy serde-facing parsing still maps non-finite numbers to null.

## Additive UTF-16 API (bounded prerequisite)

Contract: [shared JSON UTF-16 spec](../../specs/js-json.md). Model-data consumption is integrated at `f2ec514`, with the lossless timestamp adapter at `3e6cb09`; independent integrated proof and validator completion remain pending.

| Export/branch | Behavioral proof |
|---|---|
| `JsString::from_utf16`, `From<&str>`, `units`, derived `Eq`/`Hash`/`Ord` | `strings_keep_units_equality_hash_and_utf16_sort_order`: high/low/replacement distinct, pair/raw equal, astral sorts before U+E000 by code units |
| `to_utf8`, `to_utf8_lossy` | Same test: checked lone-surrogate rejection, valid pair accepted, explicit U+FFFD replacement |
| `JsValue` null/bool/number/string/array/object variants; `parse_lossless` | `lossless_json_matches_node_bytes_and_key_identity`: nested lone units, raw Unicode, controls, scalars, numeric boundary, errors |
| `JsObject` (`IndexMap` alias), `object_entries_lossless` | Same differential: exact key units and canonical-index enumeration, distinct surrogate keys, duplicate overwrite/slot order, raw/escaped equivalent emoji keys |
| `stringify_lossless`, `stringify_lossless_pretty`, shared writer | Same differential compares compact/pretty strings byte-for-byte; `serde_conversion_and_constructed_lone_units_use_shared_writer` covers constructed unmatched units |
| `JsValue::from(&Value)`, `into_serde_lossy` | `serde_conversion_and_constructed_lone_units_use_shared_writer`, `legacy_parse_replaces_before_collisions_and_preserves_original_slot`: recursive conversion and explicitly later key collisions |
| Legacy `parse`, `stringify`, `stringify_pretty`, `object_entries` | Existing six focused tests and mutation corpus reused as test modules in the new pi-js-compat tests; explicit lone-key collision ordering regression |

One parser and writer serve both representations. Legacy parse chooses replacement at string completion, before duplicate-key insertion; converting an already-lossless object later can yield different collision winners and is explicitly named lossy. `JsValue::Number(f64)` retains JavaScript binary64 values, including overflow infinities and signed zero. Non-finite values become null only at JSON serialization or an explicit serde_json::Value projection, not native typed from_value decoding. `From<&Value>` converts numbers to binary64 before stringification, matching JavaScript rounding.

Generator prerequisite (2026-10-20): `is_truthy` and fallible `to_js_string` share JSON coercion semantics, including array joining and own `toString` shadow errors. `image_generator_coercion::json_coercions_match_node_without_losing_code_units` compares Node directly. Raw `1e400` previously became null before coercion (false / "null" rather than true / "Infinity"); the focused RED established the numeric representation change, rather than declaring raw JSON invalid. Development logs: `/tmp/pi-rust-image-generator-proof/{shared-red,shared-coercion-green,nonfinite-red,nonfinite-green}.log`.

Development evidence: `/tmp/pi-rust-js-json-utf16-proof/ledger.md` records baseline
Node differential RED (nine mismatches), targeted GREEN and formatting at `850b6e8`.
Independent integration proof at `e31b4c8` passed all 11 JSON tests, shared-crate
check and all-feature Clippy. Formatting inherits the identical integrated precommit
source check; full commands/scope hashes: `/tmp/pi-rust-e31b4c8-json-date-proof/ledger.json`.
Model-data consumption is integrated at `f2ec514` and awaits independent proof.
The subsequent binary64/coercion change invalidates this historical JSON code proof;
its development RED/GREEN evidence is recorded above. No full-port claim follows.

## Proof inventory

- Legacy focused cases: 6; total lossless target: 9 (six inherited, three new).
- Legacy differential inputs: 1,617; existing split-error `toWellFormed()` normalization remains.
- New lossless differential inputs: 45; compact/pretty bytes and key units compared without normalization.
- Independent merged proof: nine focused tests plus two differential test functions, all passed at `e31b4c8`.

## JS-value boundary regression (2026-10-20)

Shared JS stringify/object enumeration is used at this module's audited observable boundary; values and parser errors are unchanged. Concrete counterexamples and per-boundary tests: [JS-value proof](../js-value-semantics-proof.md).

## Lossless JSON error text (2026-10-21)

| Export/branch | Behavioral proof |
|---|---|
| `JsonParseError::message() -> &JsString`, `into_message() -> JsString` | `js_json::error_tests::json_error_messages_match_node_utf16_units`: borrowed units equal actual Node SyntaxError message units; consumed clone equals borrowed message |
| `unexpected_token`: missing token, special sources, raw token | Same test: end-of-input, `undefined`, raw high/low surrogates, emoji's offending high unit |
| `unexpected_token_with_context`: whole source, head, middle, tail | Same test: short raw source, 10-unit cutoff through high/low pair at either excerpt edge; multiline source retained |
| `error_at`, UTF-16 position/line/column | Same test: multiline positioned error with astral key; existing 1,617-input corpus retained |
| `Display` | Same test compares `Buffer.from(error.message, "utf8").toString("utf8")`, while message-unit assertions remain lossless |

RED/GREEN evidence: `/tmp/pi-rust-repin-json-error-text/{red,green}.log`. RED showed seven real Node unit-array mismatches despite identical UTF-8 diagnostic output. Scope is error storage/construction/accessors only; parser grammar and consumers are unchanged. Final regression/mutant ledger: `/tmp/pi-rust-repin-json-error-text/REPORT.md`.

## Q18 shared serde

| Export/branch | Proof |
|---|---|
| `Serialize` / `Deserialize` for `JsString`, `JsValue` | `js_json_serde`: nested high/low surrogates, keys, valid pairs, checked UTF-8 rejection, embedded records |
| `deserialize_present` optional property adapter | `js_json_serde`: omitted versus null versus nested surrogate value |

RED: missing serde trait implementations (compile rejection). GREEN: three sandbox tests at `/tmp/pi-rust-repin-lossless-content/serde-green.log`. JSON-only serde limitation is explicit in the shared spec.

Q18 construction: `js_json!` uses the serde_json 1.0.151 token grammar with JsValue/JsObject output and lossless expression serialization. `js_json_serde::lossless_json_macro_builds_nested_values_without_serde_value_projection` covers nested arrays/objects, dynamic lone-surrogate keys, lone-surrogate values and null/bool/number output. RED/GREEN: `/tmp/pi-rust-repin-lossless-content/macro-{red,green}.log` (six shared serde tests pass).

Native JSON source prerequisite: the shared parser now consumes UTF-16 units directly. `js_json_serde::json_parse_accepts_native_utf16_source_without_replacement` covers a raw (not escaped) high surrogate inside a JSON input string. RED API mismatch and GREEN artifacts: `/tmp/pi-rust-repin-lossless-content/native-json-source-{red,green}.log`. All 18 shared serde/lossless tests pass, including the full 1,617-input legacy/error differential and lossless Node corpus; no normalization added. This enables exact JSON fragments received inside SSE strings.
