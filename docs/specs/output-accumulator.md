# Streaming output accumulator

Port `packages/coding-agent/src/core/tools/output-accumulator.ts` at the common pin.

## Contract

- Streaming UTF-8 decoding, decoded tail, full raw spill bytes and line-byte counters follow frozen append/finish/snapshot ordering. Finish is idempotent; append after finish rejects.
- Limits retain JavaScript binary64 values, including signed zero, fractions and nonfinite values. Rolling bytes use JavaScript Math.max; Buffer property lookup precedes slice-index coercion.
- Snapshot returns frozen content, full truncation metadata and optional original artifact path. Its metadata serializes through the shared lossless JSON authority.
- Default spill path resolves tmpdir at first spill and uses the supplied prefix plus eight cryptographic random bytes encoded as sixteen lowercase hex digits. The first path remains after close; it is not reopened by later persistence requests.
- Stream errors without a listener remain uncaught. While close awaits the stream, its error rejects close. Pre-repin synchronous-IO rulings do not establish parity for these branches.
- Invalid JavaScript runtime input types remain outside typed native APIs.

## Proof and remaining work

[Traceability](../porting/traceability/output-accumulator.md) distinguishes accepted current-pin numeric/decoder/metadata proofs from pending entropy, stream-error and whole-module closure work. Tests: `differential_output_accumulator`, `differential_tmpdir_consumers`, `output_accumulator`, `lossless_output_accumulator`.
