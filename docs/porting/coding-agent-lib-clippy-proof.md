# Coding-agent library clippy preparation

Verified: 2026-10-25. Source revision: `0bbbda58c` (includes parent `b0557b03f`).

Scope: behavior-preserving lint fixes in the assigned coding-agent libraries, plus settings after its lane finished. No lint suppressions, assert changes, dependency additions, network, push, deployment or delegation. Test targets are deferred by request.

## Command results

Command: `cargo clippy --offline -j3 -p <crate> --lib --all-features -- -D warnings`.

| Crate | Result | Proof |
| --- | --- | --- |
| pi-coding-agent-base | PASS | `base.final.log` |
| pi-coding-agent-data | PASS | `data.final.log` |
| pi-coding-agent-models | PASS | `models.final.log` |
| pi-coding-agent-packages | PASS | `packages.final.log` |
| pi-coding-agent-tool-support | PASS | `tool-support.final.log` |
| pi-coding-agent-ui | BLOCKED: storage dependency lints | `ui.final.log` |
| pi-coding-agent-core | BLOCKED: storage dependency lints | `core.final.log` |
| pi-coding-agent-extensions | BLOCKED: storage dependency lints | `extensions.final.log` |
| pi-coding-agent-interactive | BLOCKED: storage dependency lints | `interactive.final.log` |
| pi-coding-agent-settings | PASS | `settings.final.log` |

Logs: `/tmp/pi-coding-agent-clippy/`. The four dependency-blocked libraries each pass the same strict command with `--no-deps`; this does not establish a pass for the requested command. Their scoped logs are `<crate>.clippy-local.log`. Later parent changes to `pi-ai` and hostname boundaries invalidate transitive proof at older commits, but not these crates' unchanged local lint fixes; the exact final commands above cover the merged dependency state and expose the remaining storage blocker.

## Changes and boundaries

- Settings, packages, tool-support and UI remove redundant borrows, identity conversions, bindings or default updates. Equivalent iterator/control-flow simplifications retain evaluation order.
- Models boxes `StorageError` callback payloads, retaining messages, causes, abort classification and rejection values; names the OAuth API-key callback type. The parent approved exactly one facade test constructor adjustment for the boxed field, included in the models commit; asserts are unchanged.
- Core retains the two first-match `JsString::replace` calls through a named intermediate. Clippy's suggested array-pattern `str::replace` would not compile for this carrier and would not preserve its semantics. No lint remains skipped here.
- Extensions names two callback types and boxes the large restored-runner variant. Other edits remove redundant conversions and borrows. Interactive does the same and removes redundant complete struct updates.

No own-crate lints were skipped. Storage, tools, facade and orchestrator production sources remain outside this lane's edits. Parent merges include other lanes' changes, including the reserved hostname file; this lane did not edit it.

## Commit and integration proof

Each implementation commit had `cargo fmt --check` and `cargo check --offline -j3 --workspace --all-targets` pass before committing. Logs are `<crate>.fmt.log` and `<crate>.check.log`; their scope is that commit's source plus already implemented earlier changes. Later source changes were checked in the next crate's workspace gate.

| Commit | Scope |
| --- | --- |
| `fbc4dccf9` | settings |
| `bcd2b69c2` | packages |
| `141f325d8` | models and approved facade test constructor |
| `5fb18b61b` | tool-support |
| `199423646` | UI |
| `e05718364` | core |
| `0c6541e13` | extensions |
| `0ecc2bee5` | interactive |
| `0bbbda58c` | parent merge; final workspace/all-target check and formatting PASS |

Changed-line readability review found no remaining issues introduced by these mechanical changes. No behavioral test changes were required; only the approved test constructor changed. No runtime or test-suite pass is claimed. Source inventory is checked separately before the checkpoint.
