# Core extension UI input and inherited terminal

Baseline: `532f7d12` (read-only source lock). Native branch base: `04f51ba`.
Spec: [core UI input](../../specs/core-ui-input.md).
Development proof: `/tmp/pi-rust-core-ui-input/ledger.md`, numbered logs.
No dependencies, live configuration, providers, merge, push, model CLI, or delegation.

## API mapping

| Pinned TS statement | Native surface | Behavioral proof |
| --- | --- | --- |
| `packages/tui/src/tui.ts:47,1156-1158`: optional `wantsKeyRelease`, false when absent; focused release filtering | `ExtensionUiComponent::wants_key_release` defaults false; `ExtensionComponentAdapter` forwards dynamically | `interactive_mode::ui_tests::extension_ui_release_opt_in_reaches_real_tui`: default absent/false, press always delivered, release delivered only when opted in, through real interactive mode and TUI |
| `packages/tui/src/tui.ts:972-987`: synchronous `stop()` | `ExtensionUiTui::stop`, required backend method; owning UI thread releases active renderer before return | `extension_ui_tui_lifecycle_is_synchronous_and_force_redraws`: terminal focus-disable bytes already written immediately after stop |
| `packages/tui/src/tui.ts:912-932`: synchronous `start()` | `ExtensionUiTui::start -> Result<(), String>`; terminal startup errors remain errors | Same test: focus-enable bytes written before return; existing regular/fullscreen factory tests pass |
| `packages/tui/src/tui.ts:998-1027`: `requestRender(true)` | `ExtensionUiTui::request_render_force`; existing `request_render()` retains false behavior | Same test flushes queued request without forcing its own renderer, requiring unchanged concrete header text to repaint |

Only the forced-true entry point is added, matching the existing Rust interactive
TUI naming. The examples need no stop options or arbitrary force flags. Backend
lifecycle calls, like fixed-cell construction, require the owning live UI thread;
request-render calls remain Send+Sync queued requests. Fixture backends fail
explicitly on unused lifecycle/force operations rather than silently no-op.

## Space Invaders — done

Native: `crates/pi-coding-agent/examples/extensions/space_invaders.rs`.
Exports `space_invaders_extension`; crate-private deterministic RNG factory is
used by tests. Explicit registration only; no dynamic/autoload entry.

| Source statements / branches | Native item | `example_space_invaders` proof |
| --- | --- | --- |
| `space-invaders.ts:8-106`: constants, shields, alien types, level delay, initial high score | constants, `initial`, typed owned state | Complete fresh/restored frames and state; ended saves reset game but retain high score |
| `108-155`: constructor, saved-game pause, 50ms interval, release opt-in | `Invaders::new/start_game`, `wants_key_release` | Fresh/resumed virtual-clock games; paused release does not resume; native game opt-in mutation rejected |
| `157-210`: held movement every other tick, bounds, cooldown, two player bullets, movement/cull, shoot timer, victory | `Inner::tick` | release/held comparison x=29 vs 28; left/right edges, held/released fire, 32-tick scripts, zero-alien victory |
| `212-249`: horizontal bounds, direction reversal, drop/invasion early return, 5/10/20 speed thresholds | `move_aliens` | Left/right bounds, drop, invasion, counts 0/5/10/20/21 |
| `251-272`: ordered column selection, bottommost shooter, random column, alien bullet cap | `alien_shoot` | Shared injected RNG; duplicate columns, shooting timers and complete bullet/state frames |
| `274-304`: alien points/high score, player lives, shields, removals after all collisions | `check_collisions` | Alien hit/victory, player game-over/restart, shield destruction, out-of-range bullets, multiple simultaneous hits |
| `306-377`: paused close/resume, ESC save, Q clear, press/release movement/fire, restart/next level | `handle_input` | Complete action transcripts and snapshots, paused close does not append, restart clears saved entry, owned saves unchanged |
| `379-523`: cache/invalidation, exact ANSI sprites/header/footer, visible-width box padding and UTF-16 line padding | `render`, `render_frame`, `cell`, formatting helpers | Complete ANSI arrays at 24/64/90 columns, repeated-width cache, invalidation, paused/game-over/victory frames |
| `525-560`: disposal, command metadata/mode guard, reverse full-session save lookup | `dispose/Drop`, factory | RPC notification; saved reloads; after disposal advance 2 seconds with no renders |

All five differential tests pass. Native inputs pass through a real pi-tui host,
not directly to the game; pinned inputs pass through real TS TUI/VirtualTerminal
in the existing invaders oracle. Native game input host is thread-local across
awaits (the extension UI future is Send); renderer flushing is deliberately
separate from explicit frame capture. Core production adapter forwarding has its
own real-interactive-mode proof above. Read/write snapshot fixtures use the
already-approved isolation delta documented in [batch L](extension-examples-l.md).

Source bugs retained and classified: three simultaneous hits with one life can
produce lives=-2; `String.repeat` then throws `RangeError: Invalid count value:
-2`. Native throws the same named error without poisoning the state lock, and
the complete error transcript matches TS. Paused resume and key-state changes
do not increment the render version until the timer; restart retains cooldown
and player-movement counters, as source does. Narrow frames are not truncated.
Tests use source-generated integer game states, not forged/malformed saves.

## Interactive shell — done

Native: `crates/pi-coding-agent/examples/extensions/interactive_shell.rs`.
Exports `interactive_shell_extension`; crate-private environment reader permits
isolated tests without changing process-global environment.

| Source statements / branches | Native item | `example_interactive_shell` proof |
| --- | --- | --- |
| `interactive-shell.ts:27-128`: default list, env additions/exclusions, JS trim/lowercase, start/tab/last-pipe matching | `DEFAULT_COMMANDS`, `is_interactive` | Detection cases: defaults, exact/prefix near misses, tab asymmetry after pipe, exclusions/additions, Unicode/JS whitespace |
| `130-155`: lowercase literal `i ` / `i\t`, bypass normal handling, non-TUI result | factory's `user_bash` handler | Actual native/pinned loader+runner; forced/non-forced/RPC transcripts and exact replacement results |
| `157-179`: stop, clear, inherited spawnSync, env shell or `/bin/sh`, start, force true, done(status), empty component | `run_inherited`, empty component | Real subprocess exits 0/7, signal and spawn failure => null, empty shell, lifecycle order/force/done; inherited stdin/stdout/stderr and screen-clear bytes asserted in a test subprocess |
| `181-195`: success/error strings, null coalesced exit code, false cancel/truncate | handler result | Entire result objects compared to pinned source |

Four tests pass (one is an explicitly gated subprocess worker). No command ever
loads a live provider or user extension directory. Commands in tests are bounded
shell exits, signal-self, and a concrete read/printf stdio probe. The source
intentionally ignores `spawnSync.error` and uses status=null for failed spawn or
signal; native preserves that outcome, not a new execution fallback. Shell cwd
and inherited environment remain the host's, not event.cwd.

`lone_surrogate_projects_to_replacement_at_string_api_boundary` supplies UTF-16
D800, asserts Rust projection to U+FFFD, and compares detection against explicit
TS `toWellFormed()` projection at that same String boundary. This is the
[approved exclusion](../../specs/compatibility.md#out-of-scope), not raw surrogate
parity. Ordinary oracle scenarios are unnormalized.

## RED/GREEN, mutation and compatibility proof

- API RED: four missing methods (01); native example REDs: missing modules (04,07).
  Negative-life behavioral RED: 18. API GREEN includes both new tests (32);
  acceptance GREEN: Invaders 5/0 (22), shell 4/0 (30).
- Seven rejected mutations: production release forwarding (10), force dispatch
  (23), stop (24), start (25), game release opt-in (26), held-left release (27),
  shell force argument (28). All restored byte-for-byte. Initial force mutant
  survived because the test forced rendering itself (11); test corrected to
  flush a stable concrete frame, then mutant rejected (23).
- All 51 `example_*` targets constructing either changed trait (including
  transitive embedded support modules) pass (22,29-31). Remaining changed
  fixture targets `default_footer_extension`, `extension_examples_editors`,
  `llama_ui` pass (33). Existing assertions were not weakened.
- Entire changed-file `interactive_mode::ui_tests` attempted: 57 passed; three
  hung, triggering cancellation after auto-backgrounding (32). Separate bounded
  probes with both edited source files replaced temporarily by untouched
  `04f51ba` reproduce all three hangs (34-36). Files restored byte-for-byte;
  no leftover test process. These are baseline verification blockers, not fixed
  by this slice: `command_restart_always_restarts_the_process_and_propagates_errors`,
  `entry_appended_renders_registered_custom_entries`,
  `render_session_entries_renders_persisted_custom_entries`.
- Formatting checked before every commit. No check, Clippy, workspace suite,
  delegation, or unsandboxed oracle/binary execution. Existing synchronous
  custom-factory completion returns without calling component.dispose in the
  Rust host; this slice returns an empty resource-free component and does not
  claim to resolve that separate host behavior.
