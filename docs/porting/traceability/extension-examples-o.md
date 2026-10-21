# Extension examples O — Doom overlay helpers

Pinned source: `532f7d12`, `packages/coding-agent/examples/extensions/doom-overlay/`.
Worktree: `/home/osso/.worktrees/pi-rust-ext-examples-o`; branch `port/ext-examples-o`.
Merged master `04f51ba` before implementation, as instructed. No core APIs changed.

## Capability matrix

| Source | Native file under `crates/pi-coding-agent/examples/extensions/doom_overlay/` | State / proof |
| --- | --- | --- |
| `doom-keys.ts` | `doom_keys.rs` | Done; two pinned helper differential tests, including raw UTF-16. |
| `wad-finder.ts` | `wad_finder.rs` | Done; private real-filesystem differential, fixture Fetch only. |
| `doom-engine.ts` | `doom_engine.rs` | Partial; fixture-module ABI/frame/input differential. Generated JS/WASM loading remains absent. |
| `doom-component.ts` | Not shipped | Stopped on absent key-release opt-in. |
| `index.ts` | Not shipped | Stopped on blocked component dependency; no inert command registration. |

These helpers do not register extensions. Tests embed actual shipped files with
`#[path]`. There is no native command/UI composition proof because the sole
extension entrypoint is stopped, not replaced with a fake extension. Nothing is
autoloaded. A host can embed the helpers explicitly; `WadFinder::new` takes the
native module directory, cwd and HOME in place of TS module/process globals, and
`ensure_wad_file` accepts the existing `pi_ai::request::Fetch` boundary (the host
may pass `pi_ai::http::default_fetch()`).

## Exact stop condition

Pinned `doom-component.ts:54`: `wantsKeyRelease = true`. Its input handler
`:83–101` needs releases to call `engine.pushKey(false, key)`. The native
`crates/pi-coding-agent/src/core/extensions/types.rs:262–286`
`ExtensionUiComponent` has input/render/focus/dispose methods but **no key-release
opt-in**. The interactive adapter at
`crates/pi-coding-agent/src/modes/interactive/interactive_mode.rs:2522–2547`
forwards input but does not override the underlying Component release opt-in.
No terminal-input subscription, manual release simulation or new core API was
substituted. The component's constructor/resume, 35 FPS timer/error exit,
half-block renderer/footer and disposal (`:13–132`) remain unported together.

Pinned `index.ts:13` imports that component and `:53–55` constructs it in
`ctx.ui.custom`. Its registration, mode guard, WAD selection, persistent
engine/reuse/resume, notifications and reset-on-error (`:18–74`) remain unported.
**Custom overlay mode is not itself missing:** native
`types.rs:454–457` has `overlay`/`overlay_options`, and `:570–575` accepts options
through `ExtensionUiContext::custom`. This index does not call nested
`showOverlay`, `isFocused` or `onHandle`; those known gaps are not blockers here.
The stopped component still prevents a faithful index port. Do not wait for the
separate core key-release slice (main-thread instruction); revisit in a later
slice after that API lands.

## U1 zero-padded gzip correction

| Pinned behavior | Native correction | Proof |
|---|---|---|
| `wad-finder.ts:47` Node gunzip accepts a valid IWAD gzip followed by ten zero bytes | `download_wad` decodes complete gzip members with buffered `GzDecoder`, stopping at zero padding after a member; concatenated-member support retained | `example_doom_overlay_helpers::absent_home_directory_wads_and_concatenated_gzip_match_pinned`, added `zero-padded-gzip` case: exact returned path, GET calls and IWAD bytes compare to pinned TS |

U1 confirmed as a defect, not a waiver: old native result was null with no WAD;
pinned TS returned the downloaded WAD. `wad-RED.log` and `wad-mutation.log`
reproduce that mismatch; restored target passes 7/7 (`wad-final.log`). Ledger:
`/tmp/pi-rust-batch-n-fixes/ledger.md`. No dependency or coverage-status change.

## Source → native → behavioral tests

All tests live in `crates/pi-coding-agent/tests/example_doom_overlay_helpers.rs`.
Oracle: `oracle/example-doom-helpers-oracle.ts`.

| Source lines / exports and branches | Native item | Behavioral evidence |
| --- | --- | --- |
| keys `:4–35` `DoomKeys` | `DOOM_KEYS` ordered table | `keys_match_pinned_raw_kitty_and_legacy_inputs`: every name/value/order compares with actual pinned export. |
| keys `:43–72` arrows, WASD/shift, fire, use, menu | `map_key_to_doom`, `matches_letter` | Same test: ASCII 0–127, legacy arrows, Kitty 15 codepoints × four modifiers × press/repeat/release, Kitty active/inactive. |
| keys `:74–88` ctrl/Ctrl+C, raw controls, lexical weapon selection, equals/minus | `map_key_to_doom` | Same test; multi-character `12`, `90`, `9x`, `10cheat`, `00` compare exactly. |
| keys `:90–104` Y/N, cheats, unknown/multi-unit strings | `map_key_to_doom` | Same test: accented chars, expanding lowercase `İ`, Greek, combining sequence, astral emoji and empty input. |
| keys raw UTF-16 / existing core String boundary | `JsString` input; `to_utf8_lossy` only for pi-tui key calls | `lone_surrogates_survive_mapping_with_explicit_string_core_projection`: U+D800/U+DFFF preserved as numeric Doom keys; mixed digit/surrogate input and U+FFFD compared to TS; core projection explicitly asserted. |
| WAD `:6–31` module path; `findWadFile` custom success/miss/empty, leading tilde, bundled-first, four ordered defaults, no match | `WadFinder::{new,find_wad_file,resolve}` | `wad_search_order_and_download_failures_match_pinned_without_network`: competing real files, lexical `other/..`, `~/` and source's `~user` prefix replacement. |
| WAD `:35–55` `ensureWadFile`: existing; download URL; HTTP, stream, gunzip, IWAD, write; caught errors → null | `WadFinder::{ensure_wad_file,download_wad}` | Same test: complete path/GET-call/file-bytes differential for existing files, successful gzip download, 404, network error, body error, corrupt gzip, PWAD, ASCII high-bit masking and missing destination directory. |
| engine `:10–25` `DoomModule` | example-local `DoomModule` trait | ABI seam only; no new public **core extension** API. Unused HEAP/FS return/getValue-type properties have no native counterpart. |
| engine `:27–46` construction, width/height | `DoomEngine::{new,width,height}` | `engine_wrapper_matches_pinned_memory_frame_and_input_lifecycle`: native defaults 640×400, uninitialized blank RGBA, no pre-init ticks/keys; post-init dimensions compare to TS. |
| engine `:58–60`, `:76–98` WAD injection/pre-run; module init/frame metadata | `DoomEngine::init_with_module` | Same test: real WAD bytes; complete FS/ABI trace; success and create error; initialized state/dimensions compare. |
| engine `:102–128` C argc/argv strings/pointers, create, ordered frees | `init_doom` | Same test: exact allocation addresses, byte writes, terminators, argv writes, argc/create and frees; source failure trace preserved. |
| engine `:133–173` tick, ARGB→RGBA, pushKey, initialized | `DoomEngine::{tick,get_frame_rgba,push_key,is_initialized}` | Same test: signed ARGB, framebuffer stride, alpha=255, pressed/released integers and post-failure no-ops compare. |
| engine `:47–57`, `:62–75`, `:87–90` generated JS presence/eval/locateFile/module promise/null-module errors | Not implemented | External runtime seam remains partial; no Rust JavaScript/Emscripten loader or real Doom playability is claimed. |

## Waiver and fixture boundaries

Doom `doom/build.sh`, `doom/build/doom.js`, `doom/doomgeneric_pi.c` are approved
external runtime waivers in [coverage](../coverage.md). Wrapper logic is
separable: the host supplies a `Box<dyn DoomModule>` via `init_with_module`.
This is not a default engine backend or a workaround for missing extension APIs.
Its partial status preserves the distinction between proven wrapper logic and
absent runtime loading. Native WAD read errors currently use Rust I/O strings,
not Node error identity/message formatting; this path is unproven, not claimed
as full error parity. ABI fixtures assume valid native addresses/dimensions.

The key oracle imports the unchanged pinned mapper and actual pinned pi-tui.
WAD and engine oracles transpile complete pinned modules into a VM: only
`import.meta.url`, process location/HOME, network fetch and generated JS asset
are fixture seams. `node:fs`, gzip and path logic remain real. Engine fake JS
runs the source's actual `preRun` callbacks with its fixture module. The WAD
oracle changes its own sandboxed cwd because Node `path.resolve` does not use
the VM's mock process object. It restores downloaded fixture bytes before the
native run, so both see identical initial disk state. No pinned files changed;
no network, live configs/DBs or providers touched. Every oracle process runs
inside cargo's mandatory test sandbox.

### Approved String boundary

[Compatibility policy](../../specs/compatibility.md#out-of-scope), line 177:
“Lone-surrogate input supplied directly to existing `String`-typed core APIs”.
The mapper retains raw `JsString` units. Only calls to pi-tui's existing
`matches_key` / `parse_key` UTF-8 APIs project lone units to U+FFFD. The explicit
regression also preserves source `charCodeAt(0)` results for those same units;
there is no blanket raw-string exclusion.

## Source discrepancies retained

- keys `:84`: lexical `"0" <= data <= "9"` accepts `"12"`, returning 49;
  preserved and independently asserted rather than corrected to one-digit input.
- engine `:121–128`: create failure skips every free (no `finally`); classified
  as source allocation-cleanup bug, preserved in the differential trace.
- WAD `:47`: Node ASCII masks high bits; bytes `C9 D7 C1 C4` pass IWAD validation.
  Preserved with an explicit fixture, not strengthened to strict ASCII validation.
- WAD `:15`: leading `~` is replaced even for `~user.wad` (not user-home lookup).
  Preserved. The downloader retains source's single-attempt/null-on-error contract.

## Proof ledger and limits

`/tmp/pi-rust-ext-examples-o/ledger.md`; complete logs `01-*.log` through
`06-*.log`. Commands are only targeted `cargo test -p pi-coding-agent --test
example_doom_overlay_helpers [lone_surrogates]` plus cargo formatting. Initial
RED preceded all three helper implementations; the grouped target stopped at
its first missing include (`doom_keys.rs`), so no separate engine/WAD missing-
module execution is claimed. Second RED proves the missing UTF-16 mapper input
boundary. Current code at `4c02ebe5` has **4 passing tests** (keys 2, engine 1,
WAD 1). `cargo fmt --check` passed before each commit. No broad test/check,
clippy, coverage, final acceptance or delegation was run by this implementer.

The oracle correction and unsupported-U+FFFD-key-identifier assertion correction
are recorded as failed development attempts, not product failures. Remaining
proof gaps: exhaustive Unicode lowercase, absent HOME, directory-as-WAD,
concatenated gzip members, all engine ABI failure points/reinit/missing-WAD
error formatting, and live external runtime behavior. No renderer, timer,
real-TUI release-delivery or native command proof is claimed for stopped files.
New direct dependency: exact `flate2 = 1.0.34`, already transitively locked;
purpose recorded in [dependencies](../../dependencies.md).
