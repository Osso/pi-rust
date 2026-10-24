# Find tool

Match `coding-agent/src/core/tools/find.ts` at immutable `4f9e6f8a4`. [Traceability](../porting/traceability/find.md).

## What it must do

### Numeric custom operations

- [x] Preserve optional binary64 limits through the custom glob boundary: absent uses1000; negative/fractional/subnormal/nonfinite/signed-zero values are not coerced to unsigned integers.
- [x] Pass the frozen ignore list and original pattern/search root to custom operations. Retain every returned result; do not invent a second custom-result slice.
- [x] Compare returned count against the original number; preserve numeric details and JavaScript spelling of notices, including zero/NaN/infinities. JSON wire serializes nonfinite numbers as null.

### Paths and processes

- [x] Custom result paths follow frozen POSIX string-prefix slicing or Node path.relative against the actual process cwd. Preserve literal backslashes and exact UTF-16, including frozen prefix cuts splitting a surrogate pair.
- [x] Preserve actual fd argv/numeric/pattern rewrites, native string validation before encoding, JS line/error trimming, CR/LF/CRLF and malformed UTF-8 repair at the existing process boundary. Use the [shared readline buffer](node-readline.md), retaining the existing EOF-driven capture lifetime.

### Cancellation

- [x] Abort settles through the shared first-winner observer without dropping a running custom exists/glob operation. Preserve the frozen explicit checks that prevent later calls after cancellation.
- [x] Send the frozen POSIX SIGTERM while the child remains alive, including after both stdio streams close; continue draining/reaping instead of dropping or force-killing the producer.
- [x] Preserve the frozen late-start quirk: abort during git discovery rejects the observer but does not retroactively kill the later child, which can finish naturally.
- [x] Preserve missing/throw/empty custom-operation boundaries, exact raw messages, actual exec disappearance, missing PATH, deleted ambient cwd rejection and combined byte/entry caps.

### Rendering

- [x] Reuse the prior Text in call/result slots and preserve collapsed hint spacing, expanded output, images and lossless text.
- [x] Apply source warning truthiness and original binary64 spelling: zero/NaN omit the result-limit warning; fractions/negative/infinite limits retain it, alongside byte-limit warnings in source order.
- [x] Incompatible prior components fail with frozen TypeError name/message and retained native error source; subsequent ordinary rendering reuses uncorrupted prior state.

## How it works

- [Module map and proof](../porting/traceability/find.md).

## Implementation inventory

- `crates/pi-coding-agent/src/core/tools/find.rs`: native find API, custom/fd execution and rendering.
- `crates/pi-coding-agent/src/bundled_extensions/bwrap/index.rs`: direct custom-glob consumer forwards the same numeric value in its existing payload.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_find_numbers.rs`:17 actual-frozen custom glob cases; exact callback inputs/content/details.
- `crates/pi-coding-agent/tests/differential_find_paths.rs`:8 actual-frozen custom path cases, with the same controlled process cwd on both sides.
- `crates/pi-coding-agent/tests/differential_find_process.rs`:42 actual-process cases, including signals, inherited late descendant pipe output, combined caps, numeric/pattern/stream/encoding/validation; full argv/results/errors.
- `crates/pi-coding-agent/tests/differential_find_failures.rs`:11 actual frozen operation/OS failure and cap cases.

### Abort and ordinary controls

- `crates/pi-coding-agent/tests/differential_find_abort.rs`:5 frozen gated custom-operation success/rejection/preabort histories.
- `crates/pi-coding-agent/tests/differential_find_child_abort.rs`:2 real-child SIGTERM marker cases with stdio open/closed, plus a deterministic held-git-IO late-start/natural-exit case.
- `crates/pi-coding-agent/tests/find_tool.rs`: ordinary custom glob and real fd controls.

### Rendering

- `crates/pi-coding-agent/tests/differential_find_render_sequence.rs`:19 frozen frames, component identity reuse, exact text/warnings/images, incompatible-component errors and subsequent state reuse; LS/shared raw-path regressions retained.

## Known gaps (current cycle)

- [ ] Final current-revision package check/scoped clippy after the whole symbol/branch closure commit. Global coding-agent lint debt remains parent-owned.

## Out of scope

Unrelated tool modules, network/deployment, Node/V8/libuv oddity emulation and invalid JavaScript runtime input types, under the approved project policy.
