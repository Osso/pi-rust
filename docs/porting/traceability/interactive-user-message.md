# components/user-message.ts

Frozen `4f9e6f8a4`, `coding-agent/src/modes/interactive/components/user-message.ts:1–68` fully compared with `crates/pi-coding-agent/src/modes/interactive/components/user_message.rs`.

| Source branch | Native | Proof |
|---|---|---|
| constructor :17–24 (text, theme, padding) | `new` with exactly those inputs | five userMessage differential cases; removed old-only ignored transformer argument and callers |
| `setOutputPad` :26–29 | `set_output_pad` | configured-padding native test; padding differential cases |
| `setInTransit` :31–36 no-op on unchanged state, rebuild on change | `set_in_transit` | same-instance true/true/false/true/false exact frozen frames |
| `rebuild` :38–57 box padding/background and Markdown options | native Box/Markdown | heading/list/narrow and normal differential frames |
| :47 dim in transit, userMessageText otherwise | native color captured at rebuild | transition differential frames |
| :52–54 muted ⧗ sending… row only while in transit | native Text row | RED absent pending row before port, complete frozen frame comparison |
| `render` :60–67 OSC133 A/B/C on nonempty frame | `render` | stable-height/zone native test and complete differential frames |

## Proof ledger

Artifacts `/tmp/pi-rust-repin-lossless-interactive/`:
- `q13-user-transit-red.log`: actual frozen comparison fails (missing dim and sending row).
- GREEN transition/raw/native proofs pending post-commit execution.

No Markdown transformer surface exists in this frozen module. The native ignored compatibility argument and obsolete oracle transformer construction are removed, not retained as a fallback. Text, styles, OSC composition and output stay `JsString`; raw test now crosses normal/in-transit/normal and padding/invalidation transitions. No scalar projection.

Native invalidate rebuilds its owned Markdown/Box to refresh theme state; the inherited frozen Container invalidates its children. Current frame equivalence is asserted by the raw/native tests; no callback or additional external state is involved.
