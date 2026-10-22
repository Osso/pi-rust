# agent-core shell output — common pin

Frozen `packages/agent-core/src/harness/utils/shell-output.ts:1–148`, 4f9e6f8a4. Rust `harness/utils/shell_output.rs`.

| Frozen lines / symbol | Behavior and proof |
|---|---|
| 1–20 options/result/error | Native output and path; frozen result fields only. Removed old returnExecutionErrors/progress options and extra metadata. Timeout error and abort process tests. |
| 22–33 `sanitizeBinaryOutput` | Preserves all units except specified controls/U+FFF9–FFFB. Retained sanitizer differential corpus; 20 raw-unit utility matrix cases include unpaired units. |
| 35–84 capture/write setup | Ordered asynchronous create/append chain; append/temp options carry abort to environment. Large-output/single-oversized-chunk real process tests assert full spill file. |
| 86–106 `onChunk` | Spill threshold counts raw UTF-8 bytes before sanitizing, CR removal; rolling retention drops whole chunks, not a byte suffix. `raw_bytes_trigger_full_output_before_sanitization` RED→GREEN. Sanitized callback text test. |
| 108–148 exec/finalize | Tail truncation; await queued writes; fullOutputRequested itself marks truncated; aborted result returns no exit code, other execution errors propagate. Seven process-boundary tests. |

DISCREPANCY preserved: `outputBytes` in frozen :94–98 actually counts UTF-16 units, so rolling retention is not byte-limited. One oversized chunk is retained whole until final tail truncation. Frozen NodeExecutionEnv ignores appendFile/createTempFile abort even though FileSystem declares these options; Rust likewise forwards the options through the interface and does not add Node checks.

Proof: seven `shell_output_capture` tests GREEN after the raw-byte spill RED; native utility differential includes sanitize. Artifacts `/tmp/pi-rust-repin-delta-agent-core/shell-red.log`, `utils-green.log`, `utils-native-green.log`.

Additional common-pin proof: `differential_shell_capture` runs 13 deterministic environment cases through frozen `executeShellWithCapture`: CR/control sanitization, raw-byte spill, whole-chunk retention, line-only spill, create/append failures, callback failures, write-error precedence over callback errors, timeout, aborted execution, already-aborted signal, and callback-error precedence over abort. It compares complete result/error fields, captured file content and callback text; no internal call-count assertions. `shell-matrix.log`: GREEN. These complement seven real subprocess tests. No unapproved module gap remains.
