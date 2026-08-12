# Differential port system

Each Rust slice is compared with the real function from the pinned TypeScript worktree.

1. `docs/porting/source-lock.json` identifies the exact source path and commit.
2. A narrow TypeScript oracle imports the selected source module through its pinned `tsx` executable.
3. Rust integration tests start one oracle process and exchange LF-delimited JSON objects.
4. The same fixtures run through the typed Rust API.
5. Tests compare normalized success values or error kinds and messages.
6. `docs/porting/traceability/` records proof for every translated statement and branch.

Differences are not resolved inside the translation. They are first classified as Rust bugs, TypeScript bugs, approved deviations, or underspecified behavior.
