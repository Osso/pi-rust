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
