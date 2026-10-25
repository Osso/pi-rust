# Strict test-target Clippy gate

Verified: 2026-10-25. Rust revision: `d1fea2df0959bafaa5577f387b23abbe817ddef1` on `repin/agents-core`. Parent library-lint work was merged through `d71a47ae9` before the remaining unit-test edits.

## Final results

Each row passed this exact command, substituting its crate name:

```text
cargo clippy -j3 -p <crate> --tests --benches --examples --all-features --no-deps -- -D warnings
```

| Crate | Result |
| --- | --- |
| pi-coding-agent-base | PASS |
| pi-coding-agent-data | PASS |
| pi-coding-agent-models | PASS |
| pi-coding-agent-packages | PASS |
| pi-coding-agent-settings | PASS |
| pi-coding-agent-tool-support | PASS |
| pi-coding-agent-storage | PASS |
| pi-coding-agent-tools | PASS |
| pi-coding-agent-ui | PASS |
| pi-coding-agent-core | PASS |
| pi-coding-agent-extensions | PASS |
| pi-coding-agent-interactive | PASS |
| pi-orchestrator | PASS |
| pi-coding-agent | PASS |

No skipped lints, new lint suppressions, lint configuration changes, or removed behavioral assertions. Production behavior is unchanged. Edits remove redundant conversions and borrows, move inline test modules, simplify fixture expressions, name complex test types, bound a test mutex guard lexically, and move constant regex construction outside a fixture loop.

## Shared support layout

The facade now declares 21 shared modules once at each integration target root. Test modules import those declarations instead of loading the same source again. This includes the shared example form and Doom helpers. The equivalent AI/TUI suite-root layout is retained. The JS-compat suite that imports a facade test has the corresponding facade oracle root declaration.

The layout change is one coherent 567-file commit (`e37d0dec1`): aliases and their target-root declarations must land together to keep all targets compilable. Other facade changes were committed in batches of about 20 files. Three attributed oracle declarations were corrected in the next commit.

## Proof ledger

- Each change commit passed `cargo check -j3 --workspace --all-targets` and `cargo fmt --check` before commit. Captured logs: `/tmp/pi-test-clippy/workspace-check*.log` and `/tmp/pi-test-clippy/fmt*.log`.
- Final Rust scope above: all 14 requested crate commands passed. Logs: `/tmp/pi-test-clippy/final-pi-coding-agent-<suffix>.log`, `/tmp/pi-test-clippy/final-pi-orchestrator.log`, and `/tmp/pi-test-clippy/facade-final.log`.
- The changed JS-compat test root also passed `cargo clippy -j3 -p pi-js-compat --all-targets --all-features --no-deps -- -D warnings`; log: `/tmp/pi-test-clippy/js-compat-final.log`.
- Final workspace compilation and format proof: `/tmp/pi-test-clippy/workspace-check-layout-final.log` and `/tmp/pi-test-clippy/fmt-layout-final.log`.
- `python3 scripts/source-inventory.py`: PASS, 686 unique rows, 186679 lines, 661 done and 25 approved gaps; no inventory mismatch.

Readability review covered the changed expressions and module layout. No additional readability changes were needed. Clippy and workspace check compile the behavioral assertions; runtime test suites were not executed as part of this lint gate. Documentation-only changes after the Rust revision do not invalidate these proofs.
