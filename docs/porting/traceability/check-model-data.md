# check-model-data.ts

Source: pinned `packages/ai/scripts/check-model-data.ts`. Rust: `crates/pi-ai/src/bin/check-model-data.rs`. [Contract](../../specs/model-data-tools.md); [validator mapping](model-data-tools.md).

| Source branch | Rust | Proof |
|---|---|---|
| Script-relative TS package root; no arguments | Authorized native adaptation: run without arguments from Rust repository root; validate `crates/pi-ai/data/providers` | `native_checker_defaults_to_rust_layout_without_source_files_or_arguments`, isolated Rust-shaped temporary tree, source files removed |
| Expected generated identity and API grouping | `native_model_data_structure` from existing compiled `providers::models_data::{builtin_provider_ids,provider_models}` | Native positive fixture plus pinned source checker differential over identical data |
| Success console.log, exit 0 | unchanged success branch | `checker_exit_stdout_stderr_match_actual_pinned_script_on_fixtures` |
| Catch validation error, exact message + hydration guidance, exit 1 | unchanged failure branch | same real pinned script differential, exact full stdout/stderr/code |

## Authorized layout decision

2026-10-20 user question: “The original catalog checker expects TypeScript import files; Rust stores its catalog as JSON in data/providers. Should the port check actual Rust catalog by default, or original TS layout?” Answer: “Check Rust catalog (Recommended)”.

Concrete metadata evidence: `crates/pi-ai/src/providers/models_data.rs` embeds 40 provider JSON files from `crates/pi-ai/data/providers`; no native TS aggregator/shards exist. Compiled model IDs/APIs supply expected structure independently of the files being validated. No source-layout fallback, arguments, flags, refresh, runtime changes or catalog writes. Original TS-layout helper exports retain their explicit contract and are not used by this binary.

## Proof ledger

| Revision / scope | Command | Result / artifact |
|---|---|---|
| `f139cff` + native fixture regression | `cargo test -p pi-ai --test model_data_tools native_checker_defaults -- --nocapture` | RED exit 2 usage vs expected 0; `/tmp/model-data-tools-native-red.log` |
| `f139cff` + native implementation/test diff | `cargo test -p pi-ai --test model_data_tools -- --nocapture` | GREEN 15/15, 0 ignored; `/tmp/model-data-tools-native-green-1.log` |

Existing source-layout historical proof remains in the validator ledger; binary argument-only behavior was replaced, not preserved. No broad/check/clippy/readability/deployment gates run; parent owns final verification.

## Existing catalog state

Static inspection: snapshot has 40 provider files; its checked-in manifest lists 39 hashes, omitting `kimi-coding.json`. This is a pre-existing stale-catalog discrepancy, not a suppressed diagnostic or implementation gap. Tests copy snapshot bytes read-only and generate manifests only inside temporary fixtures. No real catalog refresh is authorized.
