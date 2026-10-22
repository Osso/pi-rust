# pi-messages — removed at common pin

Frozen `4f9e6f8a4`: `packages/ai/src/api/pi-messages.ts` absent. Whole frozen package search for `piMessagesApi`, `streamPiMessages`, and `pi-messages` returns no matches; no moved/renamed equivalent. Frozen `types.ts` KnownApi and `compat.ts` builtin registry exclude it.

Deleted Rust API, standalone and differential tests, oracle, public/root/compat exports and builtin registration. Removed module-specific lossless HTTP test, retaining shared lossless tests for frozen-backed APIs. Registered builtin APIs now number nine in source order: RED registry assertion → GREEN 10 compat tests. Public index tests adapted for removed exports/registry count, not weakened to permit drift.

No coding-agent consumer imports this API. Old-only Radius config still emits its API tag and will be removed in its own module deletion slice; it does not load/dispatch the removed implementation. No migration or fallback added. Deletion inventory: [removed sources](../removed-rust-sources.json).

Proof/revision ledger: `/tmp/pi-rust-repin-delta-ai/ledger.md`, revision `76b33dd2` + deletion slice.
