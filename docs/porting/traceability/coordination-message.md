# coordination-message.ts

Frozen `4f9e6f8a4`, `coding-agent/src/modes/interactive/components/coordination-message.ts:1–30` fully compared. Native `src/modes/interactive/components/coordination_message.rs`.

| Source | Native | Proof |
|---|---|---|
| constructor :9–13 retains ordered child references and hidden callback | owned Container + callback | plain/rich differential frames; native raw child identity |
| render :15–17 hides lines, not components | callback checked each render | false/true/true/false/false frozen frames; hidden raw child invalidation/reveal |
| setExpanded :19–23 immediate CustomMessageComponent children only | native Any downcast hook | rich differential updates custom state while hidden, shows retained expanded output |
| setInTransit :25–29 immediate UserMessageComponent children only | native Any downcast hook | rich differential updates user state while hidden; generic Text unchanged |
| inherited Container API/invalidate | Deref/Component forwarding | native child identity/raw output after invalidation |

Interactive integration follows frozen `interactive-mode.ts:4801–4900`: classify original AgentMessage before rendering, retain existing prefix children, wrap only newly added suffix, read current hide setting from callback. Native Container retains component handles; replacing its plain child vector reproduces source splice without reconstructing message children. Whole interactive-mode re-pin is not claimed.

Artifacts `/tmp/pi-rust-repin-lossless-interactive/`:
- RED `q13-coordination-component-red.log`: prior plain Container renders a hidden child.
- RED `q13-coordination-transcript-red.log`: actual native chat exposes extension coordination while hide setting is enabled.
- Post-commit1a2e612a: `q13-coordination-component-green.log`2/2 (plain/rich differential +raw identity); `q13-coordination-transcript-green.log`1/1 actual chat consumer.

Custom/User `as_any_mut` hooks only expose their actual concrete type for source instanceof dispatch. No alternate message representation, scalar text conversion, or hidden-child disposal.
