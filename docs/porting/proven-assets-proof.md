# Proven-asset reconciliation evidence

Scope: 13 rows selected by the bounded 2026-10-20 follow-up; no classification of other inventory rows. Base: `448b5983c236eba60f990a25f445d01939fbefcd`. Original source remains [pinned](source-lock.json) at `532f7d12cd19235081bd4d6e425bde185652d86f`; inspected read-only.

## Proof ledger

Read existing `/tmp/pi-rust-448b598-verification/all.log`; no test/build/check/clippy commands run for this reconciliation. Evidence below covers base production/test code, unchanged by these docs. Later documentation edits do not invalidate it; production/test/asset changes require reassessment. Full suite: **5,087 passed / 12 failed**, not a passing gate (two bundled-command failures; ten missing-reference failures). Selected passes do not override those failures.

| Existing test target | Exact passing test | all.log line | Observable proof |
|---|---|---:|---|
| `pi-ai/tests/bedrock_provider.rs` | `provider_module_and_lazy_override_match_the_typescript_exports` | 1047 | External public-module import streams `Hello`, checks stop reason, then observes lazy override error. |
| `pi-ai/tests/bedrock_converse_stream.rs` | `stream_simple_adjusts_thinking_budget` | 1033 | Underlying simple-stream request behavior; not a separate package-forwarder execution. |
| `pi-coding-agent/tests/differential_export_html.rs` | `differential_export_from_file_matches_byte_for_byte` | 3860 | Whole HTML string equality against pinned TS for dark/light/default exports. |
| `pi-coding-agent/tests/export_html.rs` | `exports_file_sessions_with_embedded_session_data` | 4532 | Export file, CSS and decoded session data. |
| `pi-coding-agent/tests/export_html.rs` | `exports_live_sessions_with_prompt_and_tools` | 4533 | Live-session output. |
| `pi-coding-agent/tests/differential_theme.rs` | `theme_loading_matches_typescript` | 4314 | Theme-loading differential, not schema-asset distribution. |
| `pi-coding-agent/tests/theme.rs` | `loads_color_modes_and_global_theme_state` | 6583 | Built-in values/global state through fixture-package loading. |
| `pi-coding-agent/tests/theme.rs` | `validates_theme_json_and_names` | 6584 | Theme validation behavior, not execution/inclusion of local JSON-schema file. |
| `pi-session-sqlite/tests/migrations.rs` | `applies_the_current_schema_once_and_records_its_migration` | 7388 | Applies embedded SQL twice, checks one migration ID and resulting tables/columns/indexes. |
| `pi-tui/tests/native_modifiers.rs` | `no_native_helper_is_available_in_the_rust_port` | 8237 | False-only Rust behavior; explicitly not macOS/Windows addon parity. |

Static audit: read both complete byte arrays for each of nine asset pairs and compare equality, recording length/SHA-256 in module traceability. Read actual `include_str!` consumers and their output/application paths. No executable asset validator added. Byte identity proves raw asset content only after inclusion is established; no browser execution claim. The schema mirror passes byte equality but fails inclusion proof, so remains `todo`.

## Traceability and result

| Rows | Module traceability | Result |
|---|---|---|
| Five HTML-export assets | [export assets](traceability/export-html-assets.md) | `todo` → `done` |
| Dark/light and schema JSON | [theme assets](traceability/theme-assets.md) | Two `todo` → `done`; schema stays `todo` |
| Initial SQL | [initial schema](traceability/sqlite-initial-schema.md) | `todo` → `done` |
| Two Bedrock package forwarders | [entrypoints](traceability/bedrock-provider-entrypoints.md) | Two `todo` → `done` |
| Two native C implementations | [native modifiers](traceability/native-modifiers.md#native-addon-implementation-rows) | Two `todo` → existing approved `gap` |

Ten rows become `done`, two become `gap`, one remains unresolved. Matrix totals remain **820 rows / 191,994 lines**. No telemetry or split-surrogate gap changed; approval remains in [AGENTS.md](../../AGENTS.md). No source bugs classified by this slice.
