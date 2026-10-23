# Sandbox profile selection

Authority: complete common-pin `modes/interactive/components/sandbox-selector.ts` and `core/permissions/presets.ts` ([lock](../porting/source-lock.json)).

- Preserve exact preset/profile names, labels, approval policies/reviewers and policy-to-preset/bypass mapping. String predicates match membership; invalid JS runtime types remain excluded by typed APIs.
- Sandbox support is exactly `platform === "linux"`. Selector construction uses the process.platform spelling from the native host; explicit-platform construction permits deterministic embedding/proof. Non-full-access profiles on unsupported platforms append ` (unsupported on PLATFORM; Linux only)`; full-access and session inheritance labels stay unchanged. Do not prohibit selecting an unsupported profile.
- Start in global scope at the current profile. Tab cycles global → project → session → global and resets the selected row to that scope's original active profile. Session scope prepends `Inherit global/project`; an absent session profile selects it. Checkmarks reflect the original profile, not unsaved navigation.
- Up/down keybindings and k/j clamp at the ends. Confirm keybinding or LF invokes the selected scope/profile callback; session inheritance emits an absent profile. Cancel keybinding invokes cancellation. Unrecognized input leaves selection unchanged.
- Preserve the static layout, help hints, dynamic borders and per-row styling. Inherit Container's live children, add/remove/clear, render and invalidation semantics; clearing the outer children does not destroy the detached list state.
- Human text and child output remain exact UTF-16. Profile/scope enums exclude invalid source union values; preset lookup returns an immutable shared record, with no frozen built-in mutation dependency.

[Selector proof](../porting/traceability/sandbox-selector.md) · [Presets proof](../porting/traceability/permissions-presets.md).
