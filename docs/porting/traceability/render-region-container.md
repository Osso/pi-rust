# render-region-container.ts

Frozen `4f9e6f8a4`, `packages/coding-agent/src/modes/interactive/components/render-region-container.ts:1–98`.
Rust: `crates/pi-coding-agent/src/modes/interactive/components/render_region_container.rs`.

| Frozen branches | Rust | Proof |
|---|---|---|
| constructor/scoped Set, 7–20 | `new`, `track_scoped_child`; insertion-ordered identity map | `scoped_membership_and_clear_match_frozen` duplicates tracking, excludes unmounted/untracked children |
| parent/child layout, 22–28, 60–87 | `place`, `render`, `place_child_region` | same differential's exact rows; `scoped_chat_updates_do_not_render_neighbors_and_retire_once` real TuiRenderer/capture-terminal write |
| lazy region creation, missing-layout false, tryRender result, 30–44 | `request_child_render` | differential + capture-terminal test: request before layout false, stable-height update true, screen changes without neighbor render |
| remove/dispose scoped only, 46–49, 89–97 | `remove_child`, `dispose_scoped_child` | both tests: repeated removal retires once, later requests false |
| clear tracks even unmounted children, clears layout, 51–58 | `clear` | differential: tracked unmounted child disposed, ordinary child not disposed, repeated clear empty |

Differential uses an unstarted-renderer boundary (no screen writes); actual successful region repaint uses real TUI and virtual terminal, not a fake renderer. RED: `test-render-region-container-red.log`; GREEN 2/2: `/tmp/pi-rust-repin-lossless-interactive/test-render-region-container-green.log`.

Chat alone uses this container (`interactive-mode.ts:488,678`); generic extension backend remains unchanged. Shared adapter preserves the typed chat container while mounting it in the compositor. Direct child-array replacement maps to `set_children` without disposal, matching frozen's mutable array behavior.
