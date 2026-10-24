# Find tool

Match `coding-agent/src/core/tools/find.ts` at immutable `4f9e6f8a4`. [Traceability](../porting/traceability/find.md).

## What it must do

- [x] Preserve optional binary64 limits through the custom glob boundary: absent uses1000; negative/fractional/subnormal/nonfinite/signed-zero values are not coerced to unsigned integers.
- [x] Pass the frozen ignore list and original pattern/search root to custom operations. Retain every returned result; do not invent a second custom-result slice.
- [x] Compare returned count against the original number; preserve numeric details and JavaScript spelling of notices, including zero/NaN/infinities. JSON wire serializes nonfinite numbers as null.
- [x] Custom result paths follow frozen POSIX string-prefix slicing or Node path.relative against the actual process cwd. Preserve literal backslashes and exact UTF-16, including frozen prefix cuts splitting a surrogate pair.
- [ ] Preserve exact fd argument, output/error, filesystem, cancellation and renderer branches across the whole module.

## How it works

- [Module map and proof](../porting/traceability/find.md).

## Implementation inventory

- `crates/pi-coding-agent/src/core/tools/find.rs`: native find API, custom/fd execution and rendering.
- `crates/pi-coding-agent/src/bundled_extensions/bwrap/index.rs`: direct custom-glob consumer forwards the same numeric value in its existing payload.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_find_numbers.rs`:17 actual-frozen custom glob cases; exact callback inputs/content/details.
- `crates/pi-coding-agent/tests/differential_find_paths.rs`:8 actual-frozen custom path cases, with the same controlled process cwd on both sides.
- `crates/pi-coding-agent/tests/find_tool.rs`: ordinary custom glob and real fd controls.

## Known gaps (current cycle)

- [ ] Complete actual-fd numeric/process boundary proof and whole symbol/branch/error/abort/renderer map; row remains open.

## Out of scope

Unrelated tool modules, network/deployment, Node/V8/libuv oddity emulation and invalid JavaScript runtime input types, under the approved project policy.
