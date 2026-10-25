# Active goal

Make the requested test, bench, and example targets pass strict Clippy without suppressions, weaker behavioral assertions, or production behavior changes. Exclude protected src directories in storage, tools, orchestrator, and facade. Verify each changed crate with the requested Clippy command; every commit must first pass cargo fmt --check and cargo check -j3 --workspace --all-targets. No agents, network, stash, push, or deployment.

## Work

- [x] Merge parent f34da978a and read AGENTS.md.
- [x] pi-coding-agent-base: strict target Clippy passes unchanged at f34da978a.
- [x] data, models, packages, settings: requested strict target Clippy passes unchanged.
- [x] tool-support: remove redundant conversions and borrow in lossless tests; strict Clippy passes.
- [ ] Check and fix storage, tools, ui, core, extensions, interactive.
- [ ] Check and fix pi-orchestrator.
- [ ] Check and fix facade in batches of about 20 test files.
- [ ] Report final SHA and per-crate proof to main.

## Proof ledger

- f34da978a: base, data, models, packages, settings requested strict target Clippy PASS (logs /tmp/pi-test-clippy/<suffix>.log).
- tool-support diff test edits: requested strict Clippy PASS; workspace all-target check PASS; cargo fmt --check PASS. Readability: only redundant wrappers removed; no behavioral assertions changed. Logs /tmp/pi-test-clippy/{tool-support,workspace-check,fmt}.log.
