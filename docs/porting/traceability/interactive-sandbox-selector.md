# sandbox-selector.ts

Source: `coding-agent/src/modes/interactive/components/sandbox-selector.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/sandbox_selector.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `SANDBOX_PROFILES` options | `options_for_scope` | differential (`sandbox`) |
| session scope prepends inherit option | `SESSION_INHERIT_OPTION` | differential two Tab presses |
| active profile checkmark | `active_profile` | differential session scope |
| scope cycle global→project→session→global | `cycle_scope` | differential |
| confirm persistent selection | `SandboxSelectorSelection::Persistent` | differential event |
| confirm session inherit (`profile: undefined`) | `SandboxSelectorSelection::Session { profile: None }` | differential event (key omitted, matching `undefined`) |
| up/down, tab, cancel | `handle_input` | differential |
| static layout + key hints | `new` | differential lines |

No gaps.
