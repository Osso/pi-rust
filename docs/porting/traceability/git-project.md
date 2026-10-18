# coding-agent git-project traceability

Pinned source: `packages/coding-agent/src/utils/git-project.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential test skipped: depends on real git repository state; proven with real temporary repositories in `crates/pi-coding-agent/tests/git_project.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `git-project.ts:4-13` | Run `git rev-parse --path-format=absolute --git-common-dir` in `cwd` with ignored stdin/stderr | `utils::git_project::read_git_common_directory`; focused common-directory test |
| `git-project.ts:10` | Non-zero status returns undefined | Focused `returns_none_outside_a_git_repository` |
| `git-project.ts:11-12` | Trim output; empty output returns undefined; otherwise resolve against cwd | Focused subdirectory test; resolution delegates to `utils::paths::resolve_path` |

## Discrepancies

- Returns `Option<PathBuf>` instead of `string | undefined`; resolution reuses the proven path module.

## Proof inventory

- Focused cases: 3 (root, subdirectory, non-repository).
