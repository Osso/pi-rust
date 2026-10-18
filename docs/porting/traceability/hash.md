# utils/hash traceability

Pinned source: `packages/ai/src/utils/hash.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `shortHash(str)` | Iterates `str.charCodeAt(i)` (UTF-16 code units), applies `Math.imul` mixing, then renders both unsigned halves in base 36, high half first. | `utils::hash::short_hash` iterates `encode_utf16()` and uses `i32::wrapping_mul`/logical shifts; `to_base36` renders unsigned halves. `hashes_concrete_strings_like_the_typescript_source`, `hashes_utf16_code_units_not_utf8_bytes`. |
| Differential oracle | 46 concrete inputs: empty, ASCII, controls, Latin-1, BMP, astral, long repeated strings, and the Copilot tool-call ID. | `tests/differential_hash_sanitize.rs` compares bit-for-bit against the pinned module through `oracle/hash-sanitize-oracle.ts`. |

## Exclusions

- None for the hashing contract. UTF-16 iteration preserves JavaScript code-unit semantics for astral characters.
