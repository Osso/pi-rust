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
