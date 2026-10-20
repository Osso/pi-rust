# coding-agent paths traceability

Pinned source: `packages/coding-agent/src/utils/paths.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `26009` executes the real pinned module across 78 cases covering local-path classification, Windows shell paths, normalization options, file URLs, resolution, relative paths, canonicalization, and file revisions. Focused tests: `crates/pi-coding-agent/tests/paths.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `paths.ts:7` | Unicode space set | `is_unicode_space`; focused Unicode-space option test |
| `paths.ts:9-20` | Input options: trim, tilde expansion, home dir, `@` stripping, Unicode spaces | `utils::paths::PathInputOptions`; focused option tests |
| `paths.ts:28-34` | Canonicalize with raw-path fallback | `canonicalize_path`; focused regular/symlink/missing/dangling tests and differential |
| `paths.ts:36-43` | Revision is `dev:ino:size:mtimeNs:ctimeNs` | `get_file_revision` via `MetadataExt`; focused format test and exact differential comparison |
| `paths.ts:50-64` | Non-local prefixes: `npm:`, `git:`, `github:`, `http:`, `https:`, `ssh:` | `is_local_path`; focused and differential cases |
| `paths.ts:67-73` | Git Bash/MSYS/Cygwin/WSL drive conversion | `normalize_windows_shell_path`; focused and differential cases |
| `paths.ts:75-100` | Normalize options, Windows conversion, `~` expansion, `file://` conversion | `normalize_path`; focused and differential cases |
| `paths.ts:102-106` | Resolve against a base directory with lexical `.`/`..` handling | `resolve_path`; focused and differential cases |
| `paths.ts:108-117` | Cwd-relative path only when inside cwd; `.` for the cwd itself | `get_cwd_relative_path`; focused traversal tests and differential |
| `paths.ts:119-122` | Relative-or-absolute formatting with forward slashes | `format_path_relative_to_cwd_or_absolute`; focused and differential cases |
| `paths.ts:124-139` | Cloud-sync attributes: macOS `xattr`, Linux `setfattr`, best effort | `mark_path_ignored_by_cloud_sync`; focused best-effort test |

## Discrepancies

- File URLs with encoded `/` (`%2F`) are rejected, matching Node `fileURLToPath`.
- File URLs with a non-local host are rejected on non-Windows platforms.
- `get_file_revision` is implemented for Unix metadata; non-Unix returns `None` (explicit typed-boundary gap).
- Windows-only runtime branches (`normalizePath` applying shell-path conversion, drive-letter file URLs, `~\` expansion) are implemented behind `cfg` but are not exercised on this Linux host; the pure conversion function is differentially proven.
- Rust paths are UTF-8 strings; non-UTF-8 OS paths are converted lossily at the boundary.

## Proof inventory

- Focused cases: 13.
- Differential request: `26009`, 78 cases.

## Literal Node resolution helper (2026-10-20)

`node_resolve(base, tail)` is public for native examples needing literal Node path.resolve semantics without general input URL/tilde normalization. `example_tool_override` compares a file:// string naming an existing file against pinned TS, including literal failed-access path and audit log. Existing resolve_path callers retain their normalization contract.
