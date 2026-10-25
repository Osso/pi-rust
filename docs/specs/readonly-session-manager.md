# Readonly session manager

Source: common-pin `core/readonly-session-manager.ts`; tightly coupled prerequisite approved2026-10-24 for [footer-data-provider](../porting/traceability/footer-data-provider.md).

- Object-valued reads return isolated owned snapshots, including each tree entry.
- Reads observe the current underlying session, not a copied session state.
- Expose exactly the frozen read surface and its explicit set/clear goal mutators; never expose the backing mutex or structural session mutators.
- Original session/storage errors propagate unchanged.

Proof: `tests/readonly_session_manager.rs` and the unmodified-source oracle. Footer override integration and final validation remain open.
