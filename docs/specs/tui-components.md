# Frozen TUI components (Q2)

Authority: [source lock](../porting/source-lock.json), commit `4f9e6f8a4`.

Text supports changing padding with cache invalidation and JavaScript whitespace emptiness. Image fallback is complete literal text, including a supplied filename, independent of hyperlinks and render width. iTerm2 encoding has no later-only `size` field. Capability detection has no Windows-console special branch. Settings search removes spaces from multi-character input; a single Space always activates the selected item, including during search.

Remaining Q2 component/deletion work is tracked in the [Q2 proof](../porting/q2-tui-components-proof.md). Cell/image dimensions remain u32 (approved numeric-domain gap); typed invalid-JS inputs remain excluded. No claim of complete component parity until the module proofs finish.
