# coding-agent git-worktree traceability

Pinned source: `packages/coding-agent/src/utils/git-worktree.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `26011` executes the real pinned module with an injected executor across 10 cases (reuse, creation, master fallback, missing base, non-repository, stderr/stdout/message error formatting, CRLF output, spaces). Behavioral tests: `crates/pi-coding-agent/tests/git_worktree.rs` includes mock cases plus real git worktree creation/reuse.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `git-worktree.ts:7-12` | `WorktreeStartupError` | `utils::git_worktree::WorktreeStartupError`; focused equality test |
| `git-worktree.ts:14-26` | Default executor runs `git` in cwd and wraps failures | `ProcessGitExec`; focused non-zero exit test |
| `git-worktree.ts:32-49` | Error text prefers trimmed stderr, then stdout, then message | `format_git_error`; differential cases 6-8 |
| `git-worktree.ts:51-61` | Executor output is trimmed; executor failures become startup errors | `git_output`; differential cases |
| `git-worktree.ts:63-69` | Parse `worktree ` lines from porcelain output, trimming and dropping empties | `parse_worktree_paths`; differential CRLF case |
| `git-worktree.ts:71-83` | Prefer `origin/main`, then `origin/master`, else a fixed error | `resolve_base_ref`; focused fallback and missing-base tests |
| `git-worktree.ts:85-113` | Target is `dirname(repo)/basename(repo)-name`; reuse an existing worktree; otherwise add from the base ref | `resolve_worktree_with_exec`; differential cases and real git test |

## Discrepancies

- The source API is async and accepts an optional executor. Rust exposes sync `resolve_worktree` (process executor) and `resolve_worktree_with_exec` (injected trait).
- `execFile` maps to `std::process::Command`; Windows `cross-spawn` behavior is not ported.

## Proof inventory

- Focused cases: 9.
- Differential request: `26011`, 10 cases.
