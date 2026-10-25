# Extension API row closure — 2026-10-25

Common source: unmodified `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.
Branch: `repin/ext-api`; worktree `/home/osso/.worktrees/pi-rust-repin-ext-api`.
Final code revision: `e5ef7ac26` (Rust code unchanged since `614ce7764`). Parent integration `de46a2789` merged before final proof. Later closure edits are documentation only.

## Closed coverage

| Frozen row | Exact audited capability | Current proof |
| --- | --- | --- |
| extensions/loader.ts | paths/cwd, manifest/discovery precedence, source metadata, factories, registration/actions, unbound errors, disabled IDs | six loader differential/native parents plus runner registration cases |
| extensions/runner.ts | event dispatch/result merging/error propagation, tools/flags/shortcuts/commands, stale/read-only/live contexts, model capture, sandbox ownership, relocation, input clone identity | 29 runner behaviors (14 actual-frozen scenarios including input_images), ten contract/boundary parents |
| extensions/types.ts | 171 exported declarations, native events/results/API/UI factories, text/JSON/error carriers and erased unions | runner/SDK consumers, ten contract parents, six raw UI/dialog tests; workspace all-target compilation |
| core/index.ts | every runtime barrel export and existing native representation of type-only exports | core_and_sdk_barrels_forward_frozen_public_contracts and extension_barrel_forwards_tool_and_source_contracts |
| resource-loader.ts | all exported/private stages, partial publication, trust bootstrap/reuse, source metadata, conflicts, warnings, overrides/reentrancy, context/rules/system prompts | 12 lossless resource parents, 23 loader behaviors, two frozen rule/context parents |
| sdk.ts | public factories/forwards, startup/restore/model/default/auth, streams/hooks/options, tools/images and multi-agent forwarding | 15 SDK behaviors/differentials plus source branch audit |
| export-html/index.ts | colors/templates/replacements/base64, live/file/custom-tool export, overrides/default paths/errors | three differential parents and four live/file/session parents |
| export-terminal.ts | sanitization/widgets/history/tool state, validation, compaction context, read-only CLI/ID/directory/settings branches | five exact ANSI/process parents |

No open realistic-input branch remains in these eight rows. Approved Rust-native extension loading, owned-value API representations and the fixed native AssistantMessage key order remain as documented in AGENTS.md. Erased JS unions are mapped to existing native structs/traits/enums rather than unused new carriers. Exceptional/adversarial/runtime gaps remain one-line module trace entries, not new APIs.

## Corrected defects

| Change | Behavioral RED | Fix / commit |
| --- | --- | --- |
| Historical session model entry overrode defaults without persisted metadata | `/tmp/ext-api3-sdk-legacy-red.log`: historical vs frozen preferred | remove old-only SDK fallback, `4de4288cd` |
| Append override read previous system prompt | `/tmp/ext-api3-resource-prompt-red.log`: null/system-1 vs system-1/system-2 | publish before append processing, `4dab4bbd8` |
| Additional theme omitted source metadata | `/tmp/ext-api3-resource-theme-red.log`: null vs local/temporary/baseDir | default source branch, `4cbbab9bc` |
| Unbound continuation silently succeeded; async actions used wrong error text | `/tmp/ext-api3-loader-unbound-red2.log` | preserve continuation Result and exact short async error text, `3cf4b9387` |
| Ordinary directory order differed from Node; stable locale ties inherited that difference | `/tmp/ext-api3-resource-trust.log`, `/tmp/ext-api3-loader-order-red.log`, `/tmp/ext-api3-theme-order-red2.log`, `/tmp/ext-api3-rule-order-red.log` | raw-byte sorting before decoding/filtering/recursion; package-manager helper, loader and resource themes/rules, `614ce7764` |

`a40853638` adds isolated-child live/file output-path proof without production changes. `e5ef7ac26` separates trust-oracle setup for readability; it retains real readdir and every assertion. No manifest or explicit-order workaround remains.

## Proof ledger

All Cargo commands use explicit worktree cwd and `-j3`. Commands below are bounded to the affected modules, not full suites. Logs contain complete stdout/stderr. There are **122 distinct passing parent tests** across the retained proof scopes, including seven package-manager resolve regressions. Nine ignored helper entries are child probes invoked by their passing parents, not skipped coverage.

| Command suffix after `cargo test -j3 -p pi-coding-agent` | Revision / scope | Result / log |
| --- | --- | --- |
| `--test sdk --test extensions -- --nocapture` | `614ce7764`, final SDK/runner code | 15+29 PASS, `/tmp/ext-api3-order-sdk-runner.log` |
| `--test suite_core_2 --test suite_core_3 --test suite_core_4 resource_loader:: -- --nocapture` | `614ce7764`, final loader code | 2+12+23 PASS, `/tmp/ext-api3-order-resources.log` |
| `--test suite_extensions lossless_extension_loader:: -- --nocapture` | `614ce7764`, final discovery/runtime code | 6 PASS, `/tmp/ext-api3-order-loader.log` |
| `--test suite_core_3 trust_bootstrap_reuses -- --nocapture` | `e5ef7ac26`, extracted oracle helper | 1 PASS, `/tmp/ext-api3-trust-final.log`; replaces only this case's earlier fixture proof |
| `--test suite_core_2 differential_package_manager::differential_resolve_ -- --nocapture` | `614ce7764`, raw directory helper shared by every discovery consumer | 7 PASS, `/tmp/ext-api3-order-package-green.log` |
| `--test suite_extensions extension_session_read_view:: -- --nocapture` | `9a612c395`; later ordering edits do not intersect contract/error/context proof | 10 PASS, `/tmp/ext-api3-final-contracts.log` |
| `--test suite_extensions raw_extension_ -- --nocapture` | `9a612c395`; later edits do not affect UI carrier shapes | 6 PASS, `/tmp/ext-api3-final-ui-types.log` |
| `--test sdk --test extensions --test export_html --test differential_export_html -- --nocapture` | `9a612c395`; retain only unchanged HTML scope here, SDK/runner renewed above | 4+3 HTML parents PASS, `/tmp/ext-api3-final-standalone.log` |
| `--test suite_modes export_terminal:: -- --nocapture` | `9a612c395`; later discovery edits do not intersect saved-history export | 5 PASS, `/tmp/ext-api3-final-terminal.log` |

The initial package regression command named a retired standalone target; Cargo rejected it without running tests. The corrected suite_core_2 command above supplies the evidence. One early evaluation failed after a captured command but before storing its result; that missing proof was regenerated and saved. Neither is treated as a production RED or passing evidence.

## Compilation, clippy, formatting and readability

- `cargo check -j3 --workspace --all-targets`: PASS, zero warnings, `/tmp/ext-api3-order-check.log`, exact final Rust code at `614ce7764`. Prior logical commits also passed the same gate. The final TS-only helper extraction does not invalidate Rust compilation.
- `cargo fmt --all --check`: PASS before every commit. No source/config change follows the final formatting proof.
- Library warning-mode clippy: `cargo clippy -j3 -p pi-coding-agent-tools -p pi-coding-agent-core -p pi-coding-agent-ui --lib` at `9a612c395`, `/tmp/ext-api3-clippy-libs.log`; ordering changes renewed with `-p pi-coding-agent-packages -p pi-coding-agent-tools --lib`, `/tmp/ext-api3-order-clippy-libs.log`. Both exit0.
- Changed-test clippy: `cargo clippy -j3 -p pi-coding-agent --test sdk --test suite_extensions --test suite_core_3 --test differential_export_html`, `/tmp/ext-api3-clippy-tests.log`; changed ordering tests renewed with `--test suite_extensions --test suite_core_3`, `/tmp/ext-api3-order-clippy-tests.log`. Both exit0. Existing warnings remain; warning locations do not overlap added/changed lines. No warning suppression added.
- Strict pinned TS no-emit check uses buildSourceTsconfig/selectTypecheckReference, all four edited oracle files, and the standing ES2024 target/lib approval for frozen TUI /v sources. PASS `/tmp/ext-api3-tscheck.log`; changed lifecycle fixture renewed after extraction: PASS `/tmp/ext-api3-trust-tscheck.log`. Biome formatting/lint PASS on all four fixtures, with the final lifecycle file rechecked after extraction.

Manual Rust/TS readability audit covered every changed production line, callback lock lifetime and new fixture. Sorting is explicit at directory ingress; no callback runs under a new lock. Named trust setup keeps the request dispatcher within the test-harness length limit. No new unused carriers, dependencies, fallback paths, warning suppressions or new readability findings. rust-code-analysis-cli is not installed; inspection was manual, not claimed automated metrics.

## Inventory

`python3 scripts/source-inventory.py`: PASS, `/tmp/ext-api3-inventory.log`; 686 unique rows, 186679 locked source lines, no missing/extra/duplicate/count mismatches. This slice closes exactly eight rows: done617/gap25/partial28/todo16. `git diff --check` and final `cargo fmt --all --check` pass. No Rust change follows the compilation/clippy/test ledger above.

## Mechanical cross-lane scope

The continuation Result change touches AgentSession's existing bound callback plus six fixture action tables only. Parent explicitly approved package-manager's read_dir_entries sorting after the actual trust RED; every discovery call uses that helper. Its only other direct readdir checks emptiness, so order cannot affect it. No unrelated package-manager algorithm changed. No push, deployment, live provider, merge into parent or subagent was used; only the pre-cleared parent-tip merge into this lane occurred.
