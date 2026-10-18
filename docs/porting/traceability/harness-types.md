# Harness execution types traceability

Pinned source: `packages/agent/src/harness/types.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `types.ts:250-263` | `ShellExecOptions` optional cwd/env/inheritEnv/timeout/abortSignal plus stdout/stderr chunk callbacks | `ShellExecOptions` with owned fields and `Arc<ShellOutputHandler>` callbacks (`Err` = thrown callback message); `harness_env` streams/callback/abort/timeout tests |
| `types.ts:265-272` | `Shell.exec` resolves to `{stdout, stderr, exitCode}` or `ExecutionError` | `Shell::exec` returns `Result<ShellExecResult, ExecutionError>`; `harness_env` exec tests and 7 bash differential cases |
| `types.ts:213-224` | `FileInfo` name/path/kind/size/mtimeMs | Existing `FileInfo` plus `NodeExecutionEnv` producers; `harness_env` metadata tests and edit/read differential fixtures |
| `types.ts:226-295` | `FileSystem` interface: absolute/join/read text+lines+binary/write/append/rename/fileInfo/listDir/canonicalPath/exists/createDir/remove/temp dir/temp file/cleanup | `FileSystem` trait with the same operations and `Option<&CancellationToken>` abort mapping; `NodeExecutionEnv` implementation and `harness_env` tests |
| `types.ts:272` | `ExecutionEnv extends FileSystem, Shell` | Blanket `impl<T: FileSystem + Shell> ExecutionEnv for T`; tools consume `Arc<dyn ExecutionEnv>` |
| `types.ts:7-9` | `Result<T, E>` discriminated union | Rust `Result`; `getOrThrow` maps to `?` |
| `types.ts:12-14` | `ok`/`err` constructors | Rust `Ok`/`Err` |
| `types.ts:17-20` | `getOrThrow` throws the failure error | Rust `?` propagation; no helper added |
| `types.ts:23-25` | `getOrUndefined` returns only object successes | Explicit typed-API exclusion: Rust `Option`/`Result` already distinguish absence |
| `types.ts:28-36` | `toError(unknown)` normalization | Explicit exclusion for the filesystem/shell layer: backend failures are already `io::Error`/`FileError`/`ExecutionError` |
| `types.ts:39-160` | Skill, prompt template, harness resources, harness tool, stream options/patch, file/execution/compaction errors | Previously ported records; `AgentHarnessTool`/`AgentHarnessStreamOptions*` remain with the unported harness orchestration layer (coverage row stays `partial`) |

## Rust boundary

- Abort parameters exist only where the pinned Node implementation observes them (`readTextFile`, `readTextLines`, `readBinaryFile`, `writeFile`, `renameFile`, `listDir`, `exec`). `absolutePath`/`joinPath` keep the interface parameter but `NodeExecutionEnv` ignores it exactly like the source.
- `createDir`/`remove` options mirror the source defaults (`recursive: true`; `recursive/force: false`).
- `createTempDir`/`createTempFile` accept the source defaults at call sites (`"tmp-"`, empty prefix/suffix) because Rust has no optional-parameter defaults.
- Error causes use `Box<dyn Error + Send + Sync + 'static>` instead of the earlier `Box<dyn Error + 'static>` so env futures and `AgentTool::execute` stay `Send`. See [harness errors](harness-errors.md).
- `FileContent::{Text, Bytes}` models the source `string | Uint8Array` write/append input.
