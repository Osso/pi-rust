# Streaming output accumulator

Port `packages/coding-agent/src/core/tools/output-accumulator.ts` at the common pin.

## Contract

- Streaming UTF-8 decoding, decoded tail, full raw spill bytes and line-byte counters follow frozen append/finish/snapshot ordering. Finish is idempotent; append after finish rejects.
- Limits retain JavaScript binary64 values, including signed zero, fractions and nonfinite values. Rolling bytes use JavaScript Math.max; Buffer property lookup precedes slice-index coercion.
- Snapshot returns frozen content, full truncation metadata and optional original artifact path. Its metadata serializes through the shared lossless JSON authority.
- Default spill path resolves tmpdir at first spill and uses the supplied prefix plus eight cryptographic random bytes encoded as sixteen lowercase hex digits. The first path remains after close; it is not reopened by later persistence requests.
- Append returns before stream-error delivery. A close listener atomically claims pending errors; without a listener, a yielding task delivers the original native error through shared uncaught authority. Capture the owning Tokio runtime at construction.
- Native synchronous failures return Result to the caller from append/finish/snapshot and their path-creation helper; they never use panic as an error carrier. Bash propagates through the existing CallbackError/tool-rejection boundary. Deferred unclaimed stream errors alone use uncaught authority.
- Constructor path validation throws synchronously before a stream exists. Retain the first raw path even when validation rejects; subsequent close resolves without reopening.
- Close resolves on the stream finish event and removes its listener before descriptor close. A later descriptor-close error is uncaught, not a close rejection. Public accumulator finish only flushes the decoder; it does not install a stream error listener.
- Invalid JavaScript runtime input types remain outside typed native APIs.

## Proof and remaining work

[Traceability](../porting/traceability/output-accumulator.md) maps the entire common-pin API and branches to numeric, decoder, metadata, raw-prefix, lifecycle and real stream-fault proofs; checkpoint status and global gate blockers remain explicit. Tests: `differential_output_accumulator`, `differential_accumulator_sequences`, `differential_accumulator_open_errors`, `differential_accumulator_path_error`, `differential_tmpdir_consumers`, `output_accumulator`, `lossless_output_accumulator`.
