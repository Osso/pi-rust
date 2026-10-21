# Extension examples Q — Doom overlay

Pinned source: `532f7d12`, `packages/coding-agent/examples/extensions/doom-overlay/`.
Native directory: `crates/pi-coding-agent/examples/extensions/doom_overlay/`.
Explicit native registry embedding only; nothing autoloads. No new dependencies.

## Scope and native boundaries

`index.rs::doom_overlay_extension` accepts host WAD acquisition (`EnsureWad`)
and external runtime initialization (`LoadEngine`). Hosts compose the existing
`wad_finder.rs::WadFinder::ensure_wad_file` with their Fetch implementation and
initialize `DoomEngine` using the supplied native `DoomModule`. These replace
module/process locations and the approved generated-JS/WASM loader waiver,
not extension UI APIs. No real Doom asset/playability claim is made.

| Pinned source | Native | Proof target / status |
|---|---|---|
| component :13–45 half-block nearest-neighbor sampling, missing-channel zero, ANSI reset | `render_half_block`, `color` | `example_doom_overlay`: complete frame comparisons at 0/1/10/33/80 columns; pending GREEN |
| component :48–79 construction/resume, release opt-in, timer tick/render/error-exit | `DoomOverlayComponent::new`, `spawn_game_loop`, `wants_key_release` | Same: resume pause pair, 27+1ms boundary, tick error, disposal; pending GREEN |
| component :82–101 Q pause/quit, releases, mapped key ordering/errors | `send_input`, `handle_input` | Same: real native TUI release filter; pinned mapper; named uncaught errors, constructor rejection; pending GREEN |
| component :104–132 aspect/min-height/footer/invalidate/dispose | `render`, `dispose`, `Drop` | Same: full ANSI, repeated/invalidation renders, no ticks after disposal; pending GREEN |
| index :18–74 registration, mode guard, WAD acquisition, reuse/new/resume, custom overlay, catch/reset | `doom_overlay_extension`, `show_game` | Same: real ExtensionRegistry/Runner and UI context, complete pinned transcript; pending GREEN |
| engine :58–60 WAD read identity/message | `DoomInitError::Wad`, shared `node_read_error_record(ReadFileSync)` | `example_doom_overlay_helpers`: ENOENT/EISDIR/ENOTDIR/ELOOP, full name/message/errno/code/syscall/path and no module calls; behavioral RED observed |

## Error and timer contracts

Post-construction pushKey errors use existing `throw_uncaught(ERROR, message)`
after releasing the engine lock. Constructor resume errors return `Err` through
the newly fallible custom factory; index catches and resets both active fields.
Tick errors alone stop the timer and call done; they are not load errors.
Node truncates the source's fractional 1000/35ms interval to 28ms; native timer
starts after that delay, never immediately. UI-local `spawn_local` retains the
non-Send done callback, and disposal aborts its task.

The approved String boundary is explicit: raw UTF-16 lone-surrogate input is
projected to U+FFFD before native String input, compared to TS `toWellFormed()`
at that same boundary. Ordinary inputs are not normalized.

## Waivers and proof limits

Generated-JS/Emscripten/WASM loading remains the approved external runtime waiver.
Existing helper proof and source discrepancies remain in
[batch O](extension-examples-o.md). Engine status stays partial until its other
recorded proof gaps (all ABI failure points/reinitialization) are closed; this
slice does not turn WAD error proof into a whole-engine completion claim.
Concurrent command invocations and real external game assets are not proven.

## Development ledger

`/tmp/pi-rust-ext-examples-q/ledger.md` and numbered logs. Core prerequisite:
6366d384 + 8712f86a. Compile RED for fallible factory; 12 extension_ui_ tests,
87 interactive_mode tests, and 28 affected example/fixture targets pass.
Doom WAD behavioral RED (06) precedes production change. Overlay missing-module
RED (08) follows correction of Rust escape typos in initial attempt (07).
Doom GREEN results pending. No clippy/check/workspace/coverage/deployment gate.
