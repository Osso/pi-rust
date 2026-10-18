# Protocol-CBOR traceability

Pinned source: `packages/protocol/src/cbor/{options,decoder,encoder,index}.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-protocol/src/cbor/{options,decoder,encoder,mod}.rs`, re-exported from `pi_protocol` by `src/lib.rs`.

Differential proof: `crates/pi-protocol/tests/differential_protocol/cbor.rs` executes the real pinned `encodeCbor`/`decodeCbor` through `oracle/protocol-oracle.ts`. Fixtures cover every RFC 8949 vector from the TypeScript test, ordered maps, Unicode/BOM text, byte strings, negative zero, safe-integer boundaries, all invalid decoder wires, and default/caller limits. Values cross the oracle boundary as tagged JSON that preserves IEEE-754 bits and map entry order.

Focused proof: `crates/pi-protocol/tests/cbor.rs` ports `packages/protocol/test/cbor/cbor.test.ts` and additionally proves typed map duplicate rejection.

## options.ts

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `options.ts:1-3` | `UINT32_BASE`, `MAX_UINT32`, and the 512 configured-depth ceiling | `options.rs` constants; `rejects_depth_above_the_configurable_maximum`; differential depth-option fixture |
| `options.ts:5-8` | Defaults: 16 MiB bytes, 1,000,000 container entries, depth 64 | `default_limits_match_the_source_schema`; differential oversized fixtures |
| `options.ts:10-18` | Optional byte/container/depth limits | `CborOptions` with `Option<u32>`/`Option<u16>`; all limit fixtures |
| `options.ts:20-26` | `CborError` identifies data failures | `CborError`; every invalid encode/decode test; differential error-name fixtures |
| `options.ts:28-29` | Fatal UTF-8 with BOM preservation | `str::from_utf8` in `decoder.rs`; BOM and invalid-UTF-8 fixtures |
| `options.ts:31-36` | Limits must be integers in range; invalid values throw `RangeError` | `CborOptionsError`; `rejects_depth_above_the_configurable_maximum`; differential `RangeError` fixtures |
| `options.ts:38-49` | Resolve defaults and validate configured values | `resolve_options`; default/configured/limit tests and differential fixtures |

Approved typed-core exclusions: negative, fractional, NaN, and above-`u32::MAX` byte/container limits are unrepresentable in `u32`; depth above 512 is representable and rejected with `CborOptionsError`.

## decoder.ts

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `decoder.ts:10-21` | Reader retains bytes, offset, and resolved options; decodes exactly one item | `CborReader`; every decode fixture |
| `decoder.ts:23-31` | Reject trailing data | `rejects_invalid_decoder_input` (`0000`); differential trailing-data fixture |
| `decoder.ts:33-39` | Depth check and major-type dispatch | `read_item`; depth-limit tests and differential depth fixtures |
| `decoder.ts:36-37` | Major 0 unsigned integers | RFC 8949 vector tests; differential vectors |
| `decoder.ts:38-41` | Major 1 negative integers; reject values outside the safe range | Vector tests; differential unsafe-negative fixture |
| `decoder.ts:42-45` | Major 2 byte strings with byte limit | Vector tests; oversized/truncated byte-string fixtures |
| `decoder.ts:46-54` | Major 3 UTF-8 text strings with byte limit; invalid UTF-8 rejects | Unicode/BOM tests; invalid/overlong/surrogate UTF-8 differential fixtures |
| `decoder.ts:55-60` | Major 4 arrays with container limit and recursive depth | Vector/limit tests; truncated/oversized array fixtures |
| `decoder.ts:61-79` | Major 5 string-keyed maps, duplicate rejection, insertion order | Ordered-map vector tests; non-string/duplicate-key fixtures |
| `decoder.ts:80-81` | Major 6 tags reject | `c000` differential fixture |
| `decoder.ts:82-83` | Major 7 simple values | Null/bool/float vector tests; unsupported-simple and break fixtures |
| `decoder.ts:88-108` | False, true, null, float64; reject non-finite and unsafe integral floats | Vector/`-0` tests; infinity/NaN/unsafe-float fixtures |
| `decoder.ts:109-110` | Break marker rejects | `ff` differential fixture |
| `decoder.ts:111-112` | Unsupported simple/float widths reject | `f7`, `e0`, `f9`, `fa` differential fixtures |
| `decoder.ts:112-117` | Indefinite lengths reject; declared lengths enforce kind-specific limits | `5f`/`7f`/`9f`/`bf` and oversized fixtures |
| `decoder.ts:119-143` | Additional-information widths 0..23, 24, 25, 26, and 27; unsafe 64-bit arguments reject | Vector tests; truncated/reserved/unsafe-argument fixtures |
| `decoder.ts:145-158` | Truncated reads reject | Every truncated fixture |
| `decoder.ts:161-168` | Input type and total byte-length limit before reading | `decode_cbor`; byte-limit test and differential fixture |

## encoder.ts

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `encoder.ts:12-80` | Growable writer with a hard maximum byte length | `CborWriter`; byte-limit tests and differential fixtures |
| `encoder.ts:83-100` | Definite-length argument encoding for all widths | RFC 8949 vector tests; differential vectors |
| `encoder.ts:102-106` | Only plain objects are maps | Typed `CborValue::Map` exclusion for Date/Map/custom prototypes |
| `encoder.ts:108-117` | Text length limit and valid Unicode scalar values | Text-limit test; Rust `&str` excludes lone surrogates |
| `encoder.ts:119-126` | Depth check | Excessive-depth test and differential fixture |
| `encoder.ts:127-131` | Null and booleans | Vector tests |
| `encoder.ts:132-145` | Finite numbers; safe integers use major 0/1; negative zero and fractions use float64 | Vector/`-0`/unsafe-number tests and differential fixtures |
| `encoder.ts:146-151` | Strings encode as text | Vector tests |
| `encoder.ts:152-159` | Byte strings with byte limit | Vector/limit tests |
| `encoder.ts:160-177` | Arrays with container limit, no holes/undefined, cycle rejection | Container-limit test; holes/undefined/cycles are unrepresentable in `CborValue` |
| `encoder.ts:178-206` | Maps reject enumerable symbol keys, omit undefined values, enforce container limit and cycles | Typed `CborValue::Map` excludes symbol keys, undefined values, and cycles; duplicate keys reject (Rust-only guard) |
| `encoder.ts:208-210` | Unsupported value types reject | Typed `CborValue` excludes bigint, symbol, function, Date, and Map |
| `encoder.ts:212-217` | Resolve options and encode one value | `encode_cbor`; all encode fixtures |

## index.ts

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `index.ts:1-9` | Export decoder, encoder, error, options type, and three defaults | `cbor/mod.rs` and `lib.rs` re-exports; crate tests import the public root |

## Discrepancies and approved deviations

- TypeScript `Object.keys` orders integer-like object keys numerically; Rust `CborValue::Map` preserves the caller's entry order. Protocol records use fixed camelCase keys, so this does not affect protocol bytes. Differential map fixtures avoid integer-like keys.
- Duplicate map keys cannot exist in a JavaScript object; Rust rejects them before encoding so it never emits a payload its decoder rejects.
- Cycles, holes, `undefined`, bigint, symbol, function, Date, Map, enumerable symbol keys, and lone-surrogate strings are unrepresentable in `CborValue`/`&str`. These are approved typed-core exclusions, not silent fallbacks.
- JavaScript checks `Uint8Array` runtime identity; Rust typed slices make non-byte input unrepresentable.
