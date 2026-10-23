# Executable availability

Frozen: `packages/coding-agent/src/utils/executable.ts` at `4f9e6f8a4`.
Rust: `crates/pi-coding-agent/src/utils/executable.rs`.

| Frozen lines | Behavior | Proof |
|---|---|---|
| 12–20 | Empty command false; options/default cwd and env; explicit executable paths resolve relative to cwd | `differential_executable::executable_lookup_matches_frozen_with_scalar_and_raw_os_cwds` |
| 22–27 | PATH search, empty component uses supplied cwd, nonempty relative component stays relative to process cwd, missing PATH uses `/usr/bin:/bin` | Same 19-case subprocess matrix, ordinary and invalid-OS-byte working directories |
| 30–32 | POSIX absolute/slash command paths; backslash is not a POSIX separator | Explicit/relative path fixtures |
| 34–42 | POSIX command names unchanged | Same matrix |
| 44–47 | First case-insensitive env key wins in property order | Differentials for `PATH`, `path`, and `Path` preceding `PATH` |
| 49–55 | Follow symlinks; require regular file and X_OK; stat/access exceptions return false | Executable, non-executable, directory, symlink, broken link, NUL and raw UTF-16 filename fixtures |
| 9–10, 24, 31, 35–41, 52 | gap: win32 PATHEXT, separators and access branches | Approved Windows-only gap, 2026-10-22 |

Commands/options/env strings retain JsString units. Native process cwd uses shared decode_node_path; process env uses the shared process_env authority. Filesystem paths encode only at syscalls, matching Node. Default cwd failures return shared NativeNodeError `uv_cwd` records; stat/access failures intentionally return false as frozen does. Typed ProcessEnv values exclude invalid JS runtime types and preserve undefined values through Option.

RED: `/tmp/pi-rust-repin-core-tools/executable-red.log` (available fixture incorrectly returned false). GREEN/check pending at implementation commit.
