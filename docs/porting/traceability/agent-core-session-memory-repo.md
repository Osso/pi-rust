# memory-repo.ts

Source: common-pin `agent-core/src/harness/session/memory-repo.ts:1–50`.
Rust: `harness/session/memory.rs` (`InMemorySessionRepo`).

`create/list/open/delete/fork` implemented against native trees and shared fork selector. Fork selector has direct frozen differential proof; repository list/order/delete/identity and fork orchestration still lack their own behavioral differential. Status: partial; compilation is not runtime parity evidence.

[Shared representation and owned-copy approval](agent-core-tree-session.md).
