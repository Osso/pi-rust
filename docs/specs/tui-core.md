# TUI core frozen compatibility

Reference: [source lock](../porting/source-lock.json), `4f9e6f8a4`.

- Built-in keybinding defaults match the 32-action frozen registry; later alternate-screen/history actions and modified viewport aliases are not defaults.
- StdinBuffer uses one timeout, default 10ms, for every incomplete sequence; paste accumulation, duplicate printable suppression and explicit flush preserve frozen event order.
- Terminal Shift+Enter normalization applies only to Apple Terminal. Protocol negotiation preserves complete input ordering. Progress clear writes OSC `9;4;0;` terminated by BEL.
- Color scheme reports accept one strictly framed report, never concatenated reports.
- Width uses frozen base-grapheme semantics, not later terminal-spacing-mark/conjunct corrections. Normalization decomposes Thai/Lao AM only, preserves tabs. Wrapping splits LF only. Truncation does not add an OSC8 close absent from frozen.

Internal later-only component/interactive helpers remain pending Q2/Q13; they are not frozen package-root exports.

## Explicit unresolved boundaries

Non-finite RGB channels/Kitty flags and negative/fractional/non-finite terminal dimensions are pending user classification, not approved semantic deviations. Native Darwin modifiers and Windows VT-console helper remain approved platform gaps. Existing Rust driver-owned scheduling/event queues, byte-offset text APIs and valid-Unicode Strings remain representation adaptations; unrestricted JS number/surrogate behavior is not claimed.

## Stateful and process boundaries

The global keybinding manager preserves shared mutation identity across returned handles; registry replacement does not mutate old holders. Debug/crash default paths resolve HOME/.pi/agent regardless of PI_CODING_AGENT_DIR. PI_TUI_DEBUG emits complete diagnostic payloads under /tmp/tui; redraw-log append errors propagate like frozen. Write-log directory filenames use local YYYY-MM-DD_HH-MM-SS and pid; ignored logging failures create no missing parent.

Real terminal lifetimes preserve partial UTF8 across OS reads, reclaim the old stdin worker before restart, skip unavailable raw-mode operations for redirected stdin and extend drain idle time on raw data even if consumed as a protocol response. These contracts are proved against frozen using private sandbox PTYs/pipes, not a live terminal.

[Current proof and pending numeric domains](../porting/q1-tui-core-proof.md).

## Core identity and failure contracts

Overwidth changed-line errors restore end-of-content position, show the cursor and stop the terminal before propagating the descriptive error. RenderRegion.try_render never schedules a root render when it fails. Removing a child removes only its first identical reference. Input/color callbacks use Rc-pointer identity (JS function identity), deduplicate registration, and follow live Set iteration order. TERMUX_VERSION is dynamically read and only nonempty values enable its height-resize policy. Nested/restarted CSI extraction retains the frozen unanchored SGR match and carries its style across wraps.
