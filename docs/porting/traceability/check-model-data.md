# check-model-data.ts

Source: pinned `packages/ai/scripts/check-model-data.ts`. Rust: `crates/pi-ai/src/bin/check-model-data.rs`. [Contract](../../specs/model-data-tools.md); [validator mapping](model-data-tools.md).

| Source branch | Rust | Proof |
|---|---|---|
| Script-relative TS package root; no arguments | Compile-time Rust crate root (`CARGO_MANIFEST_DIR`), `data/providers`; independent of CWD | `native_checker_defaults_to_source_root_from_unrelated_working_directories`: read-only fixture overlay at actual compiled catalog path, no TS source files, two unrelated CWDs, success and schema-error full streams |
| Expected generated identity and API grouping | `native_model_data_structure` from existing compiled `providers::models_data::{builtin_provider_ids,provider_models}` | Native positive fixture plus pinned source checker differential over identical data |
| Success console.log, exit 0 | unchanged success branch | `checker_exit_stdout_stderr_match_actual_pinned_script_on_fixtures` |
| Catch validation error, exact message + hydration guidance, exit 1 | unchanged failure branch | same real pinned script differential, exact full stdout/stderr/code |

## Authorized layout decision

2026-10-20 user question: “The original catalog checker expects TypeScript import files; Rust stores its catalog as JSON in data/providers. Should the port check actual Rust catalog by default, or original TS layout?” Answer: “Check Rust catalog (Recommended)”.

The earlier CWD-root requirement was an inferred discrepancy, not part of this authorization. Pinned TS derives `packageRoot` from `import.meta.url`, independent of CWD. The native source-crate root preserves that default: `data/providers` under the compile-time crate root, not executable location or CWD. No environment override or alternate-path search. Explicit API callers retain `validate_native_model_data(repoRoot)`; the bin calls existing `validate_model_data_directory(&native_model_data_structure(), dataPath)` directly. Source npm hydration guidance stays verbatim until a native generator command contract is ready.

Concrete metadata evidence: `crates/pi-ai/src/providers/models_data.rs` embeds 40 provider JSON files from `crates/pi-ai/data/providers`; no native TS aggregator/shards exist. Compiled model IDs/APIs supply expected structure independently of the files being validated. No source-layout fallback, arguments, flags, refresh, runtime changes or catalog writes. Original TS-layout helper exports retain their explicit contract and are not used by this binary.

## Proof ledger

| Revision / scope | Command | Result / artifact |
|---|---|---|
| `f139cff` + native fixture regression | `cargo test -p pi-ai --test model_data_tools native_checker_defaults -- --nocapture` | RED exit 2 usage vs expected 0; `/tmp/model-data-tools-native-red.log` |
| `f139cff` + native implementation/test diff | `cargo test -p pi-ai --test model_data_tools -- --nocapture` | Historical GREEN 15/15, 0 ignored; `/tmp/model-data-tools-native-green-1.log`; CWD behavior subsequently falsified |
| `0f535e6` + source-root regression tests only | `cargo test -p pi-ai --test model_data_tools native_checker_defaults -- --nocapture` | RED 0 pass / 1 fail, 15 filtered; valid mounted fixture ignored in favor of unrelated CWD, exit 1 vs 0; `/tmp/pi-rust-checker-root-proof/red.{stdout,stderr}` |
| `0f535e6` + source-root bin/test diff, formatted | `cargo test -p pi-ai --test model_data_tools checker -- --nocapture` | GREEN 3/3, 13 filtered, 0 ignored; `/tmp/pi-rust-checker-root-proof/green.{stdout,stderr}`; 8 sandboxed bin invocations, success/error in 2 CWDs per process test, complete TS stream/status equality retained |

Linux process tests require `bwrap` and working unprivileged namespaces; missing support is a test failure, never an unsandboxed fallback. Child namespace read-only binds the isolated fixture directory over the exact compiled catalog path, unshares network, clears environment and sets private HOME/PATH. Host catalog is unchanged. Raw process diagnostics, including Node's experimental-VM warning, are retained in the artifacts. Coverage remains partial pending parent verification; machine-readable ledger: `/tmp/pi-rust-checker-root-proof/ledger.json`.

Existing source-layout historical proof remains in the validator ledger; binary argument-only behavior was replaced, not preserved. No broad/check/clippy/readability/deployment gates run; parent owns final verification.

## Existing catalog state

Static inspection: snapshot has 40 provider files; its checked-in manifest lists 39 hashes, omitting `kimi-coding.json`. This is a pre-existing stale-catalog discrepancy, not a suppressed diagnostic or implementation gap. Tests copy snapshot bytes read-only and generate manifests only inside temporary fixtures. No real catalog refresh is authorized.
