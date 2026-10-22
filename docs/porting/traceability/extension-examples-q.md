# Extension examples Q — Doom overlay
## Common-pin WAD download (2026-10-22)

Frozen wad-finder.ts:8 downloads the ibiblio raw `.wad`; :42–48 writes response bytes unchanged. Removed old gamers gzip URL, decompression and IWAD validation. Existing controlled-fetch tests reproduced both URL and saved-byte mismatches, including concatenated gzip treated as opaque bytes (`test-example_doom_overlay_helpers-392a8a65.log`). No network used.


Pinned source: `532f7d12`, `packages/coding-agent/examples/extensions/doom-overlay/`.
Native directory: `crates/pi-coding-agent/examples/extensions/doom_overlay/`.
Explicit native registry embedding only; nothing autoloads. No new dependencies.

## Native entry and waiver boundaries

`index.rs::doom_overlay_extension` accepts host WAD acquisition (`EnsureWad`)
and external runtime initialization (`LoadEngine`). Hosts compose the existing
`wad_finder.rs::WadFinder::ensure_wad_file` with their Fetch implementation and
initialize `DoomEngine` using a supplied native `DoomModule`. These replace
module/process locations and the approved generated-JS/WASM loader waiver,
not extension UI APIs. Loader discovery/evaluation/locateFile/module-promise and
null-module rejection (`doom-engine.ts:47–57,62–75,87–90`) stay at that external
runtime boundary. No actual Doom asset/playability claim is made.

## Capability and proof matrix

| Pinned source | Native | Behavioral proof |
|---|---|---|
| component :13–45 half-block nearest-neighbor sampling, missing-channel zero, ANSI reset | `render_half_block`, `color` | `example_doom_overlay`: complete frames at 0/1/5/10/33/80 columns; empty framebuffer renders black |
| component :48–79 construction/resume, release opt-in, timer tick/render/error-exit | `DoomOverlayComponent::new`, `spawn_game_loop`, `tick_and_render`, `wants_key_release` | Resume pause pair; failures on either resume key; 27+1ms first-tick boundary; tick and requestRender failures exit once; no post-disposal ticks |
| component :82–101 Q pause/quit, releases, mapped key ordering/errors | `send_input`, `handle_input` | Real native TUI release filter; press/release Q, movement/run/unknown/lexical weapon keys; errors remain named uncaught input errors, not timer exits |
| component :104–132 aspect/min-height/footer/invalidate/dispose | `render`, `dispose`, `Drop` | Full ANSI arrays, footer truncation, repeated/invalidation frames, idempotent disposal; framebuffer errors propagate without poisoning engine state |
| index :18–74 registration, mode guard, WAD acquisition, reuse/new/resume, custom overlay, catch/reset | `doom_overlay_extension`, `show_game` | Real ExtensionRegistry/Runner and custom UI context; complete pinned transcript for TUI/RPC/JSON/text, failed/successful acquisition, JS trim, same/different WAD, init/UI/resume-constructor errors and subsequent fresh initialization |
| engine :58–60 WAD read identity/message | `DoomInitError::Wad`, shared `node_read_error_record(ReadFileSync)` | `example_doom_overlay_helpers`: ENOENT/EISDIR/ENOTDIR/ELOOP, full name/message/errno/code/syscall/path and no module calls before failure |
| engine :76–98,102–128 pre-run/argv/metadata and every failing ABI invocation | `DoomModule`, `init_with_module`, `init_doom` | `example_doom_engine_failures`: oracle-derived 44-call successful-init trace; failure injected separately at every call, both fresh and during reinit; exact partial state/trace/cleanup, then successful recovery |
| engine :133–173 tick, signed ARGB conversion, key press/release, initialized state | `tick`, `get_frame_rgba`, `push_key`, accessors | Same: every runtime ABI fails independently, either framebuffer pixel read fails, exact error and later state; missing-WAD reinit preserves previous engine; original [batch O](extension-examples-o.md) full-byte framebuffer/default/uninitialized proof retained |
| keys :4–104 constants, key branches, lowercase fallback | Existing `DOOM_KEYS`, `map_key_to_doom` | `example_doom_overlay_helpers`: all 65,536 individual BMP code units plus every supplementary cased scalar; original raw/Kitty/legacy/protocol/UTF-16 differentials retained |
| WAD :6–55 search/download branches | Existing `WadFinder` | Same: genuinely absent and empty HOME; tilde-relative, absolute-tilde miss and defaults; cwd/bundled directories accepted as existing WADs; two gzip members concatenate byte-exactly; original search/fetch/error corpus retained |

## Error and timer contracts

Post-construction input/framebuffer errors use existing `throw_uncaught(ERROR,
message)` after releasing the engine lock. Constructor resume errors return
`Err` through the newly fallible custom factory; index catches and resets both
active fields. Tick and requestRender errors alone stop the timer and call done;
they are not load errors. `swallow_panics` handles the existing named-throw path
for a failing render request, matching the source timer's catch-all boundary.

Node truncates the source's fractional 1000/35ms interval to 28ms; native timer
starts after that delay, never immediately. UI-local `spawn_local` retains the
non-Send done callback; disposal aborts its task. Wall-clock jitter/stall behavior
is not a real-time scheduling guarantee in either host.

The approved String boundary is explicit: raw lone-surrogate UTF-16 input is
projected to U+FFFD before native String input, compared to TS `toWellFormed()`
at that same boundary. Ordinary inputs are not normalized. The helper mapper
retains raw BMP surrogate units, as previously proven.

## Source discrepancies retained

- Existing [batch O discrepancies](extension-examples-o.md#source-discrepancies-retained)
  remain unchanged: lexical multi-character weapons, skipped frees on create
  failure, high-bit-masked IWAD headers, and prefix-only tilde replacement.
- Any argv/free ABI failure skips subsequent frees; no `finally` was invented.
- A failed reinit does not clear `initialized` or old dimensions/frame pointer.
  Depending on the failure point, the newly assigned module coexists with stale
  metadata. All 44 failure points preserve that source state, including its
  subsequent tick/key/frame behavior; successful recovery is separately proven.

## Proof ledger and status

`/tmp/pi-rust-ext-examples-q/ledger.md` and numbered logs; commands run only
through cargo's isolation runner. No live config, provider, network, DB or pinned
source writes. No clippy/check/workspace/coverage/deployment gate was run.

- Core prerequisite `6366d384` + `8712f86a`: compile RED; `extension_ui_` 12/12,
  `interactive_mode` 87/87, and all 28 affected example/fixture targets pass.
  Source contract and adapter proof: [core UI overlay](core-ui-overlay.md).
- WAD error behavioral RED (06); missing-module RED (08). Initial attempt (07)
  also had invalid Rust escapes; corrected before production implementation.
- RequestRender error behavioral RED (11), GREEN (12). All-ABI compile RED
  (13) exposed infallible metadata/frame reads; example-local ABI signatures now
  preserve those failures. No core extension API was added for that change.
- Current scoped proof: component/index 5/5 at `6e998538` (16); helpers 7/7 at
  `4f947ec5` (15); engine failure/reinit 2/2 at `13aa69e9` (14). Later commits
  do not modify the proven engine-script branch or engine implementation.

Component, index and engine wrapper are **done**, with only the external
runtime loader waiver above. All previously listed helper proof gaps are closed.
Valid native module addresses/dimensions remain the typed ABI domain; malformed
external-memory layouts and concurrent command invocation schedules are not
covered by these fixtures.
