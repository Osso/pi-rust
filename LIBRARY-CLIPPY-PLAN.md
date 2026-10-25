# Last-library strict clippy gate

Active goal: fix strict library/binary all-feature clippy for storage, tools, orchestrator, and facade, in that order, with no behavior changes. Each crate commit must pass formatting and workspace all-target checking. Excluded: test-target lint fixes, interactive lint fixes, suppression, network, push, deployment, and delegation.

- [x] Storage: fix lints, inspect behavior-preserving diff, strict clippy, format, workspace check, commit.
- [x] Tools: same gate, separate commit.
- [x] Orchestrator: same gate, separate commit.
- [x] Facade: same gate, separate commit.

Proof ledger: /tmp/pi-library-clippy. Existing lint failures are RED evidence; no behavior changes or weakened assertions.

Storage proof (b0557b03f + storage diff): 72 initial diagnostics, all fixed with identity-conversion/borrow removals and resulting redundant-closure/field cleanup. UTF-16 carriers unchanged. Strict clippy, cargo fmt --check, cargo check -j3 --workspace --all-targets, and source inventory pass. Readability: changed lines are direct field/value expressions; no new branching or suppressions. Logs: /tmp/pi-library-clippy/storage-*.

Tools proof (26c1b5472 + tools diff): 59 initial diagnostics fixed. Identity conversions/borrows removed, color parsing borrows the same UTF-16 units, cleanup callback aliases preserve types. Command comparison uses existing JsString PartialEq<String>, which compares encode_utf16 units exactly (js_json.rs:313-325), not a lossy str projection. Strict clippy, formatting, workspace all-target check pass. Readability: simple expressions and named callback aliases; no new control flow. Logs: /tmp/pi-library-clippy/tools-*.

Orchestrator checkpoint (01acb5a22): no crate-local diagnostics with --no-deps. The exact command now reaches facade and fails on its 46 diagnostics; these are the next in-scope crate, not skipped lints. No orchestrator source change is required. Merged parent f34da978a contains interactive fixes. Workspace all-target check passes at this merge. Exact orchestrator command will be repeated after facade code changes.

Final proof (4172ed8a6 + facade diff): exact requested orchestrator command passes after facade library fixes; exact facade command also passes after its binary fixes. Facade fixes remove identity conversions/borrows, shorthand redundant fields, and flatten two equivalent short-circuit conditions. `is_option_index` names the unchanged finite/nonnegative/integral check. No semantic lint exceptions, suppressed/skipped lints, behavior changes, or assert changes.

Facade's requested `--bins` includes `tests/bin/example_rpc_demo_harness.rs`; its single redundant byte-buffer borrow was removed. No other test target was changed. Formatting, workspace all-target checking, source inventory (686 rows; 661 done, 25 approved gaps), and diff whitespace checking pass. Readability audit: direct expressions, equivalent let-chains, one descriptive boolean; no new nested flow or suppression. Logs: /tmp/pi-library-clippy/{orchestrator-final,facade-final,facade-fmt,facade-check}.log. The orchestrator proof is unaffected by subsequent facade-only binary changes. Storage and tools proofs remain valid: no later source change intersects those crates or their dependencies. No new behavioral tests: these are semantics-preserving lint fixes, with all targets checked.
