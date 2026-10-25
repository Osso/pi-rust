# Bound child sessions

- Forward abort, prompt, optional runtime coordination and transcript state to the raw child session.
- Read messages from the child session manager's current branch. Preserve message identity for current-turn result extraction.
- Retain the live model, effort, registry, scoped-model and model-cycle mutation surface after production binding.
- On disposal, await the child extension runner's session_shutdown event with reason quit. Always dispose the raw session afterwards, even if shutdown fails. A disposal failure overrides a shutdown failure.
- All runtime disposal paths await completion and propagate failures. Release runtime listeners and handles before final disposal.

[Module mapping and proof](../porting/traceability/child-session.md).
