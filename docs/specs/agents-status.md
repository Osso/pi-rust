# Agents status

- Register the `agents-status` widget on session start only when a store is supplied.
- Show active agents and terminal agents updated less than ten seconds ago. Place visible children beneath visible parents; sort siblings by lifecycle priority and then newest updated timestamp.
- Show at most five rows, with a count of hidden rows. Preserve frozen icons, theme roles, elapsed duration, column caps, whitespace normalization and width truncation.
- Refresh this component on store updates and every second while rows are visible, including the final update that hides expired rows. Dispose its timer and subscription when the component is removed.
- Use the source-equivalent targeted TUI render request, not unconditional full-widget reconstruction.

[Module proof](../porting/traceability/agents-status.md).
