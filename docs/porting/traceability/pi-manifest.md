# coding-agent pi-manifest traceability

Pinned source: `packages/coding-agent/src/core/extensions/loader.ts:593–611` (`readPiManifest`) and `core/package-manager.ts:158,533` (`readPiManifestFile`) at common pin `4f9e6f8a4` (full pin in `../source-lock.json`).

Differential proof: request `27006` reads 16 fixture paths — all four resource fields, non-string entries, empty arrays, wrong container types, invalid JSON, empty file, UTF-8 BOM, invalid UTF-8, duplicate keys, a missing file, and a directory — through the pinned module and the Rust port. Focused tests: `crates/pi-coding-agent/tests/pi_manifest.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `PiManifest` | Optional `extensions`/`skills`/`prompts`/`themes` string arrays | `PiManifest` with `Option<Vec<String>>` fields and omitted-when-absent serialization; `keeps_only_string_array_resource_fields` |
| `RESOURCE_FIELDS` | The four manifest fields | Four explicit reads in `read_pi_manifest` |
| `isObject` | Non-null, non-array object | `Value::as_object()` |
| `readPiManifest` | `null` for unreadable/invalid files, non-object `pi`, or a non-object package; otherwise the filtered manifest | `read_pi_manifest`; focused and differential cases |
| Field filter | Keeps an array only when every entry is a string | `string_array` via `collect::<Option<Vec<String>>>` |

## Exclusions

- Takes `&Path` instead of a string path.

## Discrepancies

None. The BOM, invalid-UTF-8, and duplicate-key fixtures confirm `readFileSync(..., "utf-8")` and `serde_json` agree.

## Q18 native continuation (2026-10-22)

Manifest resource arrays now contain `JsString`, parsed through the single
`JsValue` JSON authority. Input paths are native; only the filesystem read encodes.
`manifest_resource_strings_preserve_escaped_unpaired_units` covers escaped lone
surrogates in extension and skill entries. Tests remain written but unexecuted:
coding-library errors block all coding tests (extensions-red.log). Existing proof
above predates this migration and is not new-pin Q18 evidence.

### Reader consolidation and runtime-type exclusion

For valid typed manifests both source readers expose the same resource arrays: the
loader reads object-valued `pkg.pi`; package-manager reads `pkg.pi ?? null` and its
consumers access optional resource fields. Rust's reader retains all valid string
arrays, including lone-surrogate strings. Null/missing/primitive pi yields no usable
resource fields for either caller; Rust returns None (or empty manifest for an
object). The source package manager can retain arbitrary non-resource `pi` fields;
the typed Rust resource API intentionally does not expose those fields.

Non-string array elements are **excluded invalid JavaScript runtime types**, under
the August 12, 2026 typed-core decision, not parity filtering: source extension
discovery calls `path.resolve(dir, 123)` and throws uncaught; Rust's typed
`Vec<JsString>` cannot represent that input. `string_array` refuses the entire
invalid field, never a partial list. Existing malformed-input tests document this
exclusion and do not prove TS parity for excluded inputs.
