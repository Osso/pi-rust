# coding-agent paths — native checkpoint

## Current whole-module audit (2026-10-24)

- `paths.ts:105–117` invokes synchronous child-process validation. Native cloud-sync marking now returns the existing `NativeNodeError` Result from `spawn_sync`; it intentionally ignores OS spawn errors/status in the returned record, as frozen does. The sole package consumer propagates at frozen `package-manager.ts:1893`. Actual frozen NUL-argv behavioral RED: `paths-boundaries-red.log`; GREEN: `paths-cloud-nul-green.log` under `/tmp/pi-rust-repin-core-tools`.
- `paths.ts:76–77` uses Node fileURLToPath. Actual frozen `file:////host/x` yields `//host/x`; the old URL parser returned `/host/x`. The pinned url2.5.x parser applies a retired rule removing leading empty file segments. The attempted adapter also encountered obsolete drive shortening and was removed. Main approved the shared [Node file-URL authority](../../specs/node-file-url.md): `pi_js_compat::node_url` owns path states and decoding; existing base `url::Url` is used only for host/IDNA validation. No dependency or frozen source changes. Tested IDNA verdict differences may be explicit gaps under main's approval. Whole URL matrix and remaining module proofs are in progress, not accepted yet.


Frozen module: `packages/coding-agent/src/utils/paths.ts` at `4f9e6f8a4`. Rust: `utils/paths.rs`. Historical `532f7d12` proofs are not common-pin acceptance.

| Source capability | Implementation / exact proof | Missing or blocked |
|---|---|---|
| Input trim, Unicode-space replacement, `@`, tilde/home | Native `JsString` input/output/options; 96 source-wrapper outcomes include raw home/path units | Broad prior fixtures need native API migration |
| Local-prefix classification | Native JS trim and literal prefixes | No whole-platform claim |
| Resolve, cwd-relative, display-relative | Native POSIX component operations; 2,527 Node POSIX outcomes across scalar/raw segments, dot/parent/trailing/root/backslash cases | Windows path operations unimplemented, not an approved gap |
| Canonicalize / source catch-and-return-original | Node-style JS realpath walk retains native non-link components; actual filesystem binding alone encodes. Existing replacement-named files and symlink prefixes: four exact frozen comparisons | Wider realpath race/device/pipe/cache cases unproven |
| `file://` | Actual WHATWG URL boundary consumes USVString; native raw input retained until that constructor | Comprehensive URL error codes/names, Windows drive/UNC behavior unproven |
| Cloud-sync marking | Native text until actual process argv encoding | Invocation/error behavior not re-proven |
| `process.cwd()` dependency | Explicit `Result`; shared Node uv_cwd formatting; read-classification process differential compares deleted-CWD error exactly | Windows remains a recorded gap |

## Q5 shared home-error propagation (2026-10-23)

The private home_directory authority returns `Result<JsString, NativeNodeError>`, not panic_any. normalize_path maps failure into its existing PathError; resolve_path propagates it; canonicalize_path retains frozen's explicit catch-and-return-original. Mechanical consumers trust_manager::has_trust_requiring_project_resources and external_session_importer::import_external_session_alias propagate errors through existing Result boundaries. Supplied homeDir still avoids lookup entirely.

`differential_render_home` extends the HOME-unset/forced-getpwuid_r-EIO process proof to tilde normalization/resolution/canonicalization and collapsed read classification, plus real ToolExecutionComponent frames. The native child previously panicked outside renderer catch (render-home-tilde-red.log); the authority no longer panics. Broader NSS errno/return-code mismatch remains [approved](node-os.md).

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
