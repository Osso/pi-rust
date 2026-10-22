# coding-agent paths — native checkpoint

Frozen module: `packages/coding-agent/src/utils/paths.ts` at `4f9e6f8a4`. Rust: `utils/paths.rs`. Historical `532f7d12` proofs are not common-pin acceptance.

| Source capability | Implementation / exact proof | Missing or blocked |
|---|---|---|
| Input trim, Unicode-space replacement, `@`, tilde/home | Native `JsString` input/output/options; 96 source-wrapper outcomes include raw home/path units | Broad prior fixtures need native API migration |
| Local-prefix classification | Native JS trim and literal prefixes | No whole-platform claim |
| Resolve, cwd-relative, display-relative | Native POSIX component operations; 2,527 Node POSIX outcomes across scalar/raw segments, dot/parent/trailing/root/backslash cases | Windows path operations unimplemented, not an approved gap |
| Canonicalize / source catch-and-return-original | Node-style JS realpath walk retains native non-link components; actual filesystem binding alone encodes. Existing replacement-named files and symlink prefixes: four exact frozen comparisons | Wider realpath race/device/pipe/cache cases unproven |
| `file://` | Actual WHATWG URL boundary consumes USVString; native raw input retained until that constructor | Comprehensive URL error codes/names, Windows drive/UNC behavior unproven |
| Cloud-sync marking | Native text until actual process argv encoding | Invocation/error behavior not re-proven |
| `process.cwd()` dependency | Explicit `Result`; no empty-path fallback | Exact deleted-cwd error text not yet proven |

## Removed old-pin exports

Frozen paths has neither `normalizeWindowsShellPath` nor `getFileRevision`. Shell-drive conversion is removed. The revision helper remains crate-private solely for its current model-cache consumer; model-cache source alignment remains separate. Old focused/differential fixtures invoking these exports must be retired/adapted, not treated as new-pin proof.

## Development proof ledger

`tests/lossless_paths.rs` and `oracle/lossless-node-path-oracle.ts` exercise actual source modules/Node functions. The standalone manifest under `/tmp/pi-rust-repin-lossless-content/chalk-standalone/crates/probe` imports the actual Rust file and test target; unchanged mandatory runner, explicit private `PI_TEST_SANDBOX_DIR`, fake/private filesystem only. Logs there:

- `paths-red.log`: scalar-only API compiler RED.
- `paths-green-attempt.log`: behavioral mismatch for `canonicalizePath("")`; Node resolves cwd before filesystem access.
- `paths-posix-matrix.log`: two tests, 96 wrapper + 2,527 Node POSIX outcomes.
- `paths-realpath.log`: behavioral RED, existing raw input replaced before returning realpath.
- `paths-realpath-green.log`: three tests, including four actual realpath/symlink comparisons. Intermediate compile attempt needed an explicit cache value type.

Standalone output reports unused internal model-revision/helper-method warnings; not suppressed and not a warning-free project gate. Current source/caller integration, broad prior fixtures, readability/final checks and independent verification remain open. This checkpoint is not whole-module or full-port acceptance.
