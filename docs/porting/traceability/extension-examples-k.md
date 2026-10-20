# Extension examples K — tic-tac-toe

Pinned source: `packages/coding-agent/examples/extensions/tic-tac-toe.ts` at
`532f7d12` (full revision in [source lock](../source-lock.json)). Native source:
`crates/pi-coding-agent/examples/extensions/tic_tac_toe.rs`; instruction asset:
`tic_tac_toe_instructions.txt` beside it. Explicit embedding only; no discovery,
providers, credentials, live state, or automatic loading.

Snake and Space Invaders were reassigned to batch L; this batch changes neither
source coverage row and ships neither implementation nor stub for them.

## Contract / proof matrix

Tests: `crates/pi-coding-agent/tests/example_tic_tac_toe.rs`.
`scripted_games_frames_tools_errors_and_persistence_match` uses the shared B
loader/runner fixture; `persisted_message_and_tool_renderers_match_complete_ansi_lines`
uses shared A renderers and B tools. Both execute unchanged pinned TS through
`TsOracle`, inside the Cargo runner sandbox.

| TS symbol / branch | Native representation | Behavioral differential proof |
| --- | --- | --- |
| Default factory, command/tool/renderer registration | `tic_tac_toe_extension` | Command/tool names, tool descriptions/schema/snippets/guidelines, call renderers; sequential execution mode retained |
| `createInitialState`, `reconstructState` | `State::default`, `restore` | Startup, current-branch tool results, tree navigation, completed-game reset; custom entries do not restore state |
| `getWinLine`, `checkWin` | `win_line`, `check_win` | All eight win-line orientations rendered; X win, O win, draw, playing paths |
| `boardToAscii`, `getBoardDetails` | `board_ascii`, `details` | Board inspection, agent cursor coordinates, private user cursor omitted, snapshot details and append-entry values |
| `centerPad`, cell/border helpers, `renderBoard`, `renderVisualBoard`, `renderBoardSnapshot` | `center`, `cell_color`, `border_color`, `cell_content`, `paint_border`, `render_board`, `render_game` | Complete ANSI lines at widths 12/18/25/40/80; user/agent cursors, winning cells, mixed adjacent colors, static snapshots, truncation/centering |
| `TicTacToeComponent` constructor/input/update/render/invalidate | `GameComponent`, `ComponentData` | Arrow bounds, occupied human cell ignored, enter/space play, other-turn input ignored, quit/escape/restart, active and terminal frames, cached frames and invalidate |
| `BannerMessageComponent` | `Banner` | Persisted move and expanded tool messages, expanded/collapsed, absent details |
| `GameOverMessageComponent` | `GameOver` | Persisted X/O/draw banners, unknown status and absent-details defaults |
| `delay`, `ACTION_DELAYS` | `execute_action` / Tokio timer | Virtual clock: each directional move takes 300 ms; play takes zero; scripts compare elapsed values and subsequent complete frames |
| `before_agent_start` instructions | `INSTRUCTIONS` / instruction asset | Active prompt appended byte-for-byte; terminal game returns no change |
| `/tic-tac-toe`, user callback, `emitGameOverMessage` | Command handler, `user_play`, `send_over` | Non-TUI notification; name change; user move content/trigger-turn option; X/O/draw message payloads; append-entry order |
| `tic_tac_toe.execute` | `execute_action`, `move_cursor`, `agent_play` | Four clamped directions; successful play/reset; wrong turn, finished game, occupied cell error retaining cursor and saving details; state unaffected by illegal plays |
| `tic_tac_toe_see_board.execute`, tool renderers | `see_board`, `render_tool_result` | Board/status/turn output; inspection has no save; error/success prefix, empty text, expanded/collapsed, missing details |

## Source behavior and boundaries

- `reconstructState` ignores custom `tic-tac-toe-save` entries despite appending
  them after actions. It reconstructs from tool-result snapshots only. Classified
  as a preserved source inconsistency: reopening before another tool result can
  lose a human-only move. Tests exercise that boundary rather than silently
  repairing it.
- Move actions remain legal on the human turn and after game completion; only
  `play` enforces turn/status. Preserved source behavior, not a new restriction.
- State/board snapshots are deep copies; no persisted-session reference aliasing
  is required. Active component/global-state sharing is represented by `Arc`;
  reconstruction replaces global state and updates a component only on a later
  component update, as in TS.
- JavaScript stack frames are omitted. Invalid JavaScript runtime types are
  outside typed native inputs: board is 3×3 cells, cursor coordinates in 0..2,
  action is one of the five schema strings. No new dependencies or API gaps.
- Tic-tac-toe has no randomness. The oracle injects a microtask virtual timeout
  clock without changing game logic; native tests pause Tokio time and compare
  move elapsed time. No real sleeps or external model calls are required.

## Evidence ledger

Logs and command/revision scopes: `/tmp/pi-rust-ext-examples-k/ledger.md`.
`03-tic-tac-toe-red.log`: two behavioral failures with unregistered game APIs.
`05-tic-tac-toe-green-attempt.log`: two tests pass.
`06-tic-tac-toe-expanded.log`: expanded tests pass, including prompt and tree paths.
Mutation evidence and final committed-revision proof are recorded after commit.
