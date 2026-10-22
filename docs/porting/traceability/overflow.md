# Overflow — common-pin re-port

Frozen `packages/ai/src/utils/overflow.ts`, `4f9e6f8a4` (191 lines), compared whole to `utils/overflow.rs`.

| Export / branches | Frozen lines | Proof |
|---|---|---|
| 25 ordered overflow regexes, 3 exclusion regexes | 38–86 | `differential_overflow`: every regex tested against 38 strings, plus end-to-end error checks; Unicode long-s, Arabic digits and vertical-tab probes |
| `isContextOverflow`: error + exclusion priority | 147–155 | 14 behavioral tests and differential message matrix |
| successful silent overflow includes cacheRead; truthy contextWindow (including negative) | 158–164 | `negative_context_window_is_truthy_like_javascript`; differential windows null/zero/negative/positive |
| length + zero output + 99% threshold | 169–174 | Xiaomi/below-window/output behavioral and differential fixtures |
| `getOverflowPatterns`: fresh copy, exact JS regex semantics | 189–191 | actual frozen pattern matrix; uses regress rather than Unicode Rust regex engine |

Deleted old-only `range of input length should be` pattern and `isRecoverableLength`, including its tests and oracle operation. Frozen whole-tree search found no renamed equivalent. Frozen coding-agent `agent-session.ts:4825` checks only `isContextOverflow`; removed its Rust recoverable-length OR branch. Consumer regression seeds persisted history then delivers short length-stop below threshold, asserting no compaction events. Empty-history probe was insufficient (preparation returned none); final test reproduced RED at the real history boundary.

RED: 3 overflow tests; exported Unicode regex differential; seeded coding-agent consumer test. GREEN: 14 behavioral + 1 differential + 1 seeded consumer. Revision `718805ff` + slice; `/tmp/pi-rust-repin-delta-ai/ledger.md`. Full agent_session target/checks recorded separately after commit.

No module-specific gaps or frozen source bugs identified. Typed Rust cannot contain invalid JavaScript message runtime shapes (approved exclusion).
