# `components/sandbox-selector.ts`

Authority: complete 137-line [common-pin](../source-lock.json) module. Rust: `crates/pi-coding-agent-interactive/src/modes/interactive/components/sandbox_selector.rs`. [Contract](../../specs/sandbox-selector.md) · [Presets](permissions-presets.md).

| Source export / branch | Native representation | Behavioral proof |
|---|---|---|
| selection/options types; constructor | typed profile/scope options and callback enums; global initial scope, active-index lookup | 3 current profiles × 4 session states × 2 widths × 4 platforms = 96 source/native sequences |
| static layout, borders, hints and list | outer Container plus retained typed `Rc<RefCell<Container>>` list child | full rendered UTF-16 parent and inherited-child frames at widths 12/100 |
| optionsForScope | exact profile order, optional session inheritance row | every current/session combination, cycle through all scopes |
| unsupported platform label | shared `is_sandbox_platform_supported`, explicit platform constructor; default maps native OS through existing `node_platform_name` | actual frozen module with process.platform authority set to linux/darwin/win32/freebsd; default constructor regression with source host platform |
| selectedIndexForScope / cycleScope | original global/session profiles retained; tab resets active index | initial state, wrap to global, current checkmark versus unsaved navigation |
| updateList | clear list, scope label/spacer, selected arrow/color, original profile checkmark | every frame compared to actual source output |
| handleInput | shared keybindings; k/j and up/down clamp; tab; CR/LF confirm; escape cancel; unknown input ignored | event arrays and every intermediate frame, upper/lower clamping and all scope callback payloads |
| inherited Container | Deref/DerefMut plus capability delegation; public live children and add/remove/clear | render every child; append raw-unit child, invalidate visibly increments its output, remove it, clear outer children, continue scope input while detached list stays invisible |
| unmatched UTF-16 input | JsString throughout | existing raw permission-selector test, lower lone unit ignored before session-inheritance confirmation; custom appended child's U+D800 compared as raw units |

## RED and proof ledger

Artifacts `/tmp/pi-rust-repin-lossless-interactive/`:

- `test-sandbox-selector-red-1.log`: rendered inherited children were absent although source exposed and rendered them. `test-sandbox-selector-container-green-1.log`: initial Linux 24-sequence proof GREEN after replacing split prefix/list/suffix render paths with the real Container.
- `test-sandbox-selector-platform-red-1.log`: actual frozen darwin initial frame includes unsupported labels; old native host-only labels do not. Source harness supplies process.platform; native adapter later supplies the parent-approved explicit platform parameter.
- `test-sandbox-selector-green-1.log`: 1/1, all 96 expanded sequences GREEN, including parent/child frames, inherited mutations, visible invalidation, raw child units and callbacks.
- `test-sandbox-presets-green-3.log`: complete presets export and raw-unit membership proof 2/2, superseding GREEN2 after single-JsString carrier alignment. GREEN1 was only a test JSON-macro compile error, not parity evidence.
- `test-sandbox-settings-carrier-green-1.log`: actual global/project settings-file raw membership and precedence 1/1; no former output failure falsely claimed.
- `test-sandbox-selector-existing-green-1.log`: unchanged default-theme/default-host source differential 1/1, two cases.
- `test-sandbox-selector-raw-green-1.log`: existing native raw-input regression 1/1.

Six focused tests cover this checkpoint; parent owns final readability/clippy/independent gates. No new semantic deviation, and no whole-Q13 acceptance is implied.
