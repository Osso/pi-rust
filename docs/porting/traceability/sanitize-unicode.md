# utils/sanitize-unicode traceability

Pinned source: `packages/ai/src/utils/sanitize-unicode.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `sanitizeSurrogates(text)` | Removes unpaired high surrogates (not followed by a low) and unpaired low surrogates (not preceded by a high), including overlapping sequences. | `utils::sanitize_unicode::sanitize_surrogate_units(&[u16])` implements the lookahead/lookbehind rule; `sanitize_surrogates(&str)` is the typed identity API. `preserves_paired_surrogates_and_plain_text`, `removes_unpaired_high_and_low_surrogates`, `repairs_overlapping_surrogate_sequences`, `string_api_is_identity_for_valid_rust_text`. |
| Differential oracle | 4,681 unit sequences (lengths 0-4 over ASCII, boundary surrogates, paired emoji, and a non-surrogate unit) plus valid text. | `tests/differential_hash_sanitize.rs` compares exact output units against the pinned module. |

## Exclusions and discrepancies

- Rust `str` cannot contain unpaired surrogates, so `sanitize_surrogates` cannot observe the repair branch; the faithful algorithm is exposed on raw UTF-16 units. Typed core APIs intentionally exclude invalid JavaScript runtime input types.
- No source bug found.
