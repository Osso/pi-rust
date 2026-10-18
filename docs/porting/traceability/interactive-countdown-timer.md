# countdown-timer.ts

Source: `coding-agent/src/modes/interactive/components/countdown-timer.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/countdown_timer.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `new` immediate `onTick(ceil(ms/1000))` | `CountdownTimer::new` | `interactive_components::countdown_timer_ticks_once_per_second_and_expires` |
| 1s interval, decrement, `onTick` | `CountdownTimer::poll` | same test (999ms / 1000ms / 5000ms) |
| `tui.requestRender()` | `request_render` callback | same test (render counter) |
| `<= 0` → dispose + `onExpire` | `poll` expiry branch | same test |
| `dispose` clears interval | `dispose`, `is_disposed` | same test |

Representation: the TS class owns a `setInterval`; the Rust port exposes the same schedule through `poll(now)`, matching the poll-driven loader/editor animation in `pi-tui`. A single `poll` past several deadlines fires each missed tick in order, like a delayed event loop. No behavioral gap.
