# Batch A integration proof

**Historical old-pin evidence:** no results below cover 4f9e6f8a4; [current proof boundary](repin-infra.md).

At the tested revisions, runtime oracle remained `532f7d12`; type-only reference and DB-v15 deviations remain separately pinned. This batch is not whole-port completion. Baseline catalogs, live state and statistics are unchanged.

## Capability and proof matrix

| Scope | Revision / evidence | Result | Remaining |
|---|---|---|---|
| Shared numbers, dates, lossless JSON/coercion, collation, Node filesystem errors | `3e6cb09`; `/tmp/pi-rust-batch-a-foundation-verification/{REPORT.md,ledger.json}` | 47 focused tests; foundation all-target check and Clippy pass | Finite corpus/platform limits; source-faithful Date readability debt below |
| Model-data/checker including original UTF-16 timestamp units | `3e6cb09`, same ledger | 17 focused tests, including exact pinned checker streams/status and process watchdog | Later generator wiring not covered by this compilation proof |
| Protected oracle/typecheck runner | `e72f323` + `6405d13`, unchanged scope through `d7e677a`; `/tmp/pi-rust-batch-a-oracle-verification/{REPORT.md,ledger.json}` | 20 Node tests and every registered downstream typeproof pass; full command exit 0 | No Rust workspace claim |
| Commands/header/SDK/custom-Anthropic examples | Integrated through `3961631`; `/tmp/pi-rust-batch-a-examples-verification/{REPORT.md,ledger.md}` | 20 independent focused tests pass with explicit private state | Default capture selector RED2 → development GREEN2 using private TMPDIR; final gate pending. Manual readability observations recorded; metrics output capped |
| Image/text catalog generators | `abc0058` / `d7e677a` checkpoints | Development proof only | Final text receiver cases, independent generator tests/lint and workspace gate |

Foundation command: `cargo test -p pi-js-compat --test number --test differential_number --test differential_number_bits --test date --test differential_date_parse --test js_json_lossless --test differential_js_json_lossless --test image_generator_coercion --test image_generator_collation --test differential_locale_compare --test node_fs_errors -- --nocapture`; caller command: `cargo test -p pi-ai --test model_data_tools -- --nocapture`. Additional commands: `cargo fmt --check`, `cargo check -p pi-js-compat --all-targets`, `cargo clippy -p pi-js-compat --all-targets -- -D warnings`.

Oracle command: `python3 scripts/test-sandbox.py -- ./run-tests.sh oracle`. Cargo's build/driver phase stays outside that namespace; compiled tests enter the mandatory Cargo runner. [Protected-runner fixes and preserved negative assertions](oracle-gate.md#protected-invocation).

## Proof limits and readability disposition

Foundation corpus: 4,346 same-bit number samples; 1,920 Date.parse evaluations; 45 lossless and 1,617 legacy JSON inputs; 16 coercions; 256 UTF-16 collation signs; eight real filesystem failures. Counts are bounded evidence, not exhaustive runtime equivalence. Generic serde spelling is numeric-semantic, not byte-exact JSON.stringify. Legacy JSON diagnostics retain documented split-surrogate normalization; new lossless cases do not. Date text comparison helpers lack a separate response-length assertion; actual oracle maps one result per input, with no observed missing result.

Audit completed; readability is not clean. New source-faithful V8 parser debt: `Scanner::scan` cognitive/cyclomatic 25/24, `parse` 58/42 (own body), `parse_iso` 62/46; local-time resolver spans 35 lines. No behavioral mismatch or unnecessary Rust control flow was demonstrated. Named extraction suggestions and inherited debt remain in the foundation report; metric-driven refactoring is deferred, not silently waived or reported fixed.

Workspace all-target/all-feature gates and current isolated startup/print/RPC/interactive proof remain open. Historical `c732b1e` workspace proof does not cover this batch.
