# Bundled extensions B re-pin proof

Reference: unmodified `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`. No live providers or real host sandboxes.

## Proof ledger

| Scope / revision | Command | Result / log | Invalidated by |
|---|---|---|---|
| Baseline `d23a14459`, remote compact | `cargo test -j3 -p pi-coding-agent --test suite_core_2 differential_openai_remote_compact::` | RED guidance; `/tmp/ext-b-compact-red.log` | Compact fixes |
| New selected-model test, pre-fix | same target, `differential_openai_remote_compact::codex_compaction` | RED forced terra model; tool transcript | Compact fixes |
| Header fix + oversized pinned test, pre-pin-fix | same target, `differential_openai_remote_compact::` | RED 500k pinned output; `/tmp/ext-b-compact-pinned-red.log` | Pin fix |
| Remote compact implementation commit (this change) | `cargo test -j3 -p pi-coding-agent --test suite_core_2 differential_openai_remote_compact::` | GREEN 3/3; `/tmp/ext-b-compact-green.log` | Relevant source/fixture changes only |
| Remote compact implementation commit (this change) | `cargo fmt --all --check` | PASS | Rust edits |

## Scope

Nine assigned source rows: bwrap backend/index, claude-bash-hook, claude-memory-enrich, codex-image-generation, codex-usage, codex-web-search, default-footer, openai-remote-compact. Module traceability contains branch coverage and explicit gaps. Focused Clippy follows implementation; no full-suite run is authorized.

## Enrich lifecycle

- RED baseline registration oracle: `/tmp/ext-b-enrich-oracle-red.log`; missing shutdown handler.
- RED real child integration: `/tmp/ext-b-enrich-lifecycle-red.log`; both children start concurrently.
- GREEN implementation commit (this change): `cargo test -j3 -p pi-coding-agent --test claude_memory_enrich_lifecycle`, 1/1, `/tmp/ext-b-enrich-lifecycle-green.log`.
- GREEN frozen oracle: `cargo test -j3 -p pi-coding-agent --test bundled_extensions_context claude_memory_enrich_matches_typescript`, 1/1, `/tmp/ext-b-enrich-oracle-green.log`.
- Mechanical fallout: new standalone target in `crates/pi-coding-agent/Cargo.toml`, because the integration mutates process-global env. No new dependency.

## Claude bash hook

- RED `suite_extensions claude_bash_hook_extension::`: missing registration gate and wrong timeout error; `/tmp/ext-b-hook-red.log`.
- GREEN same command: 10/10; `/tmp/ext-b-hook-green.log`.
- GREEN `cargo test -j3 -p pi-coding-agent --test suite_tools differential_claude_bash_hook::`: 1 test / 12 frozen cases; `/tmp/ext-b-hook-oracle-green.log`.
- Proof covers the hook implementation commit (this change), invalidated only by relevant changes.

## Hosted image generation and web search

- RED new `image-model-team` frozen case: `/tmp/ext-b-image-red.log`.
- GREEN `cargo test -j3 -p pi-coding-agent --test suite_tools differential_codex_hosted_tools::`, 1/1; `/tmp/ext-b-hosted-oracle.log`.
- GREEN same target filter `codex_hosted_tools::`, 13/13 (includes differential due substring filter); `/tmp/ext-b-hosted-behavior.log`. This filter should not be repeated.
- GREEN `cargo test -j3 -p pi-coding-agent --test suite_core_1 differential_codex_hosted_abort::`, 1/1; `/tmp/ext-b-hosted-abort.log`.
- Proof covers hosted implementation commit (this change); relevant edits invalidate it.

## Bwrap

- RED empty env differentials: `/tmp/ext-b-bwrap-env-red.log`; GREEN `/tmp/ext-b-bwrap-env-green.log` (2/2, suite_core_1 filter `differential_bwrap::matches_typescript`).
- RED status/custom shell: `/tmp/ext-b-bwrap-status-red.log`; GREEN `/tmp/ext-b-bwrap-status-green.log` (2/2, suite_extensions filter `bwrap_repin::`).
- RED worker-null/zero-notice: `/tmp/ext-b-bwrap-worker-red.log`; GREEN `/tmp/ext-b-bwrap-worker-green.log` (8/8, extensions --lib filter `bwrap::index::operation_tests::`).
- Commands use `cargo test -j3 -p <crate> --test <suite> <filter>` (or --lib). All proofs cover bwrap implementation commit (this change). No actual host sandbox.
- Temporary index detached-job/registry gap is closed by the real-process proof below.

## Usage (implementation in progress)

- RED HTTP raw text and busy report regressions: `/tmp/ext-b-usage-behavior-red.log` (2 failures); GREEN 15/15 `/tmp/ext-b-usage-behavior-green-1.log`.
- RED frozen missing-percent case `/tmp/ext-b-usage-oracle-red.log`; RED empty-metered-feature case `/tmp/ext-b-usage-oracle-empty-red.log`.
- Targets: `suite_extensions codex_usage_extension::`, `suite_core_2 differential_codex_usage::differential_codex_usage`; commands use `cargo test -j3 -p pi-coding-agent --test <target> <filter>`. Relevant usage changes invalidate prior usage proof only.
- Parent event authority merged at 6ecbb4c5d from 1a02ec893.

### Usage closure

- RED fallback credential lookup `/tmp/ext-b-usage-auth-red.log`; GREEN `cargo test -j3 -p pi-coding-agent --test codex_usage_auth_boundary`, 1/1, `/tmp/ext-b-usage-auth-green.log`.
- GREEN `cargo test -j3 -p pi-coding-agent --test suite_core_2 differential_codex_usage::`, 5/5 including raw bidirectional DTO/text oracle, all-account bridge/credentials; `/tmp/ext-b-usage-all-green.log`.
- Proof covers usage closure commit (this change). HTTP/payload/report behavior proof at f8724a65d remains valid except credential lookup, now covered above.
- Mechanical fallout: native map fixture adapters and standalone auth target registration in Cargo.toml; no dependencies or cross-lane algorithms.

## Default footer

- RED frozen ordered UI trace: `cargo test -j3 -p pi-coding-agent --test default_footer_extension background_compaction_status_lifecycle_matches_frozen`, `/tmp/ext-b-footer-lifecycle-red.log`.
- GREEN full focused target `default_footer_extension`: 13/13, `/tmp/ext-b-footer-behavior-green.log`.
- GREEN `suite_interactive differential_default_footer::`: original matrix 1/1, `/tmp/ext-b-footer-oracle-green.log`.
- GREEN new raw full-render test `suite_interactive differential_default_footer::footer_render_keeps`: 1/1, `/tmp/ext-b-footer-raw-green.log`.
- All cover footer implementation commit (this change); relevant edits alone invalidate them. Oracle fixture change adds lifecycle invocation, not a transformed source.

### Bwrap index closure

- RED `cargo test -j3 -p pi-coding-agent --test bwrap-detached-lifecycle`: no detached result details, `/tmp/ext-b-bwrap-detach-red.log`.
- GREEN same command: installed full-access extension forwards store/lifecycle/registry, registry detaches real runner, lifecycle reaches Completed, output.log is exactly `readydone`; `/tmp/ext-b-bwrap-detach-green.log`.
- Standalone harness owns internal-runner dispatch; it does not invoke bwrap, a real sandbox, or providers. Mechanical new test target in Cargo.toml.
- Proof covers bwrap index closure commit (this change). No realistic detached-job gap remains.

## Final targeted gate

- `eceb9cc11`: `cargo clippy -j3 -p pi-coding-agent-extensions --lib` PASS; `/tmp/ext-b-clippy-extensions.log`. 116 existing extension warnings, zero on lane-changed lines (each diagnostic intersected with the lane diff). No warning suppression added.
- Same revision: `cargo check -j3 -p pi-coding-agent-extensions --lib` PASS; `/tmp/ext-b-check-extensions.log`.
- Same revision: fixture Clippy `cargo clippy -j3 -p pi-coding-agent --no-deps` with explicit targets suite_extensions, suite_tools, suite_core_1, suite_core_2, suite_interactive, default_footer_extension, claude_memory_enrich_lifecycle, codex_usage_auth_boundary, bwrap-detached-lifecycle: PASS; `/tmp/ext-b-clippy-fixtures.log`. Zero warnings intersect lane-changed fixture lines. Existing unrelated warnings remain.
- Readability: manual changed-line review of nine assigned modules, new process fixtures and changed TS oracle. `rust-code-analysis-cli` is unavailable (ENOENT), so no automated complexity claim is made. Native orchestration functions retain existing longer frozen control flow; new helpers/tests have explicit names and bounded nesting. No new abstraction, fallback or suppression.
- Footer oracle strict typecheck found unknown dynamic-export types (including the pre-existing component access); explicit fixture function signatures now type the same runtime imports. Uses the pinned compiler/source config with scoped ES2024 target/lib per oracle-gate.md. No source patch or weakened assertion. Type-only fixture annotations do not invalidate runtime/Rust proof.

### Final state

All nine assigned rows are `done`; each module retains only explicit approved/runtime exclusions in its traceability, with no pending realistic-path gap. The temporary bwrap detached-job gap was fixed, not waived.

- `2d5c70edb`: protected pinned-compiler strict typecheck of `oracle/default-footer-oracle.ts` PASS; `/tmp/ext-b-footer-types-green.log`. The command creates a temporary config with `buildSourceTsconfig`, scoped ES2024 target/lib, then runs the frozen `node_modules/typescript/bin/tsc --project <temporary config>` inside `python3 scripts/test-sandbox.py`.
- `python3 scripts/source-inventory.py`: PASS, 686 unique rows / 186679 lines, no missing/extra/duplicate/count drift; `/tmp/ext-b-inventory-final.log`. Documentation-only ledger updates do not invalidate this proof.
- `cargo fmt --all --check` passed before each lane commit. Working tree clean at final checkpoint; no push/deploy, live provider or real host sandbox was run.
- Independent verification is the parent integration gate; this lane obeyed the explicit no-agent instruction and ran only bounded checks.
