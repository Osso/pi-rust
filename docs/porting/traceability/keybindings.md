# keybindings.ts frozen traceability

Reference: immutable `4f9e6f8a4`, `packages/tui/src/keybindings.ts` (249 lines).
Rust: `crates/pi-tui/src/keybindings.rs`. [Q1 proof](../q1-tui-core-proof.md).

| Frozen export/function/branch | Rust mapping | Proof |
|---|---|---|
| Keybindings, Keybinding, KeybindingDefinition(s), KeybindingsConfig | named ids, public definitions, ordered configuration pairs | 12 unit tests + source differential |
| TUI_KEYBINDINGS | ordered 32-action `tui_keybindings` table | complete resolved-key differential |
| normalizeKeys | first-occurrence deduplication | repeated keys, empty overrides and default cases |
| constructor/rebuild | definition filtering, ordered user claims, conflicts only for multiple user claimants, no default eviction | seven concrete source configurations |
| matches/getKeys | key matcher and copied key list | units + resolved source corpus |
| getDefinition | borrowed immutable definition | known/unknown definition unit cases |
| KeybindingConflict/getConflicts | deep-copied conflict records | source conflict ordering and dedup cases |
| setUserBindings/getUserBindings/getResolvedBindings | rebuild and copied configuration/key lists | unit + source resolved/conflict output |
| setKeybindings/getKeybindings, lazy initialization | process-global manager with Arc-owned immutable definitions and Arc/Mutex mutable state | direct frozen global identity differential |

Frozen defaults exclude later historyPrevious/historyNext/altScreen actions and ctrl+home/end/pageUp/pageDown aliases. The old defaults produced RED before removal.

`KeybindingsManager::clone` is a shared handle, not a snapshot. Mutations through one holder affect all holders and the originally installed manager. Replacing the registry leaves old holders attached to the old manager. Direct frozen comparison covers shared mutation, reset and replacement; RED `/tmp/q1-global-red.log`, GREEN `/tmp/q1-global-green.log`.

Typed representation: string-or-array key unions become Vec; configuration objects become ordered pairs; resolved values are key lists even for a single key. Rust callers extend definitions explicitly rather than TypeScript declaration merging. Invalid JS runtime input types are excluded by the typed API. No shared-global clone discrepancy remains.
