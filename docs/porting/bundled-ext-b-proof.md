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
- Index stays partial: explicit unimplemented full-access detached-job/registry forwarding, recorded in traceability.

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
