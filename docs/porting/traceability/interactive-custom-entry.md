# components/custom-entry.ts

Frozen: `4f9e6f8a4`, `packages/coding-agent/src/modes/interactive/components/custom-entry.ts:1–89`.
Rust: `crates/pi-coding-agent/src/modes/interactive/components/custom_entry.rs`.

| Frozen symbol / branch | Rust | Proof |
|---|---|---|
| options/sessionId/requestRender, 12–22, 49–58 | `CustomEntryComponentOptions`, `EntryRenderOptions` | `differential_custom_entry::custom_entry_replacement_cleanup_and_disposal_match_frozen` (present/empty output) |
| constructor/hasContent/setExpanded, 24–40 | `new`, `has_content`, `set_expanded` | same differential; repeated identical expansion does not rebuild |
| invalidate children then rebuild, 42–45 | `Component::invalidate` | exact generation/invalidation event sequence in same differential |
| per-generation cleanup Set, 50, 56 | shared insertion-ordered callback list, Rc identity deduplication | registering the identical callback twice retires it once |
| renderer exception box, 60–68 | `catch_panics`, `renderer_error_component` | `custom_entry_renderer_error_matches_frozen`: exact ANSI/UTF-16 rows including unpaired D800/DC00 |
| replacement invocation before old cleanup/disposal, 70–78 | `rebuild`, `dispose_rendered_component` | exact three-generation event order, present/empty output |
| cleanup then optional child disposal, 81–88; explicit clear, 47–50 | `dispose_rendered_component`, `Component::dispose` | double disposal is idempotent; empty renderer still owns cleanup; render after disposal empty |
| raw entry payload | `CustomEntry`, `JsString` output | `raw_leaf_components::custom_entry_renderer_keeps_native_payload_when_expanded` |
| mounted scoped callback, interactive-mode.ts:4777–4801 | weak component identity + chat region binding; dispose invisible entries | `render_region_container` capture-terminal proof covers region writes/retirement; mode integration has all-target compile proof, not a whole-mode TS differential |

Development proof: `/tmp/pi-rust-repin-lossless-interactive/test-custom-entry-renderer-red.log` (uncaught renderer panic before implementation), `test-custom-entry-lifecycle-red.log` (missing options/disposal), `test-scoped-entry-green-4.log` (2/2 entry differential tests).

No newly approved deviations. Rust-native renderers report native throws through `UncaughtError`; optional component disposal is represented by the approved default no-op trait hook. Generic extension TUI render routing is unchanged.
