# Active goal

Make the requested test, bench, and example targets pass strict Clippy without suppressions, weaker behavioral assertions, or production behavior changes. The parent released the storage, tools, orchestrator, and facade src directories after its library-lint work merged. Every change commit must first pass cargo fmt --check and cargo check -j3 --workspace --all-targets. No agents, network, stash, push, or deployment.

## Work

- [x] Merge parent f34da978a and read AGENTS.md.
- [x] Merge the parent's completed library-lint fixes through d71a47ae9.
- [x] Pass all 12 pi-coding-agent component crates with the requested strict test-target command.
- [x] Pass pi-orchestrator with the requested command.
- [x] Fix facade examples and tests in batches of about 20 files.
- [x] Compile shared support once per target, following the AI/TUI layout. The 567-file layout commit is atomic because shared aliases and target roots must change together.
- [x] Pass facade strict test-target Clippy and recheck pi-js-compat all-target Clippy after its shared-fixture root update.
- [x] Record the proof ledger in docs/porting/test-target-clippy-proof.md.

## Proof ledger

The detailed ledger and final per-crate results are in docs/porting/test-target-clippy-proof.md. Rust proof scope: d1fea2df0959bafaa5577f387b23abbe817ddef1. Subsequent documentation-only changes do not invalidate this scope. No lints were skipped.
