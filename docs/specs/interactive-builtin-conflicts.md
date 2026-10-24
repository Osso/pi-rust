# Interactive builtin command conflicts

Frozen `interactive-mode.ts:772–785,2020–2026` at the [common pin](../porting/source-lock.json) defines conflict warnings.

- Match resolved commands against builtin names exactly, preserving registration order.
- An unchanged invocation says `Skipping in autocomplete.`; an alias says `Available as '/<invocationName>'.`.
- Warning paths retain each command's source-info path, including arbitrary UTF-16.
- Loaded-resource extension diagnostics append builtin conflicts after command diagnostics and before shortcut diagnostics.

`rank5_builtin_conflicts_render_frozen_registered_command_diagnostics` compares actual native loaded-resource frames with actual frozen `showLoadedResources`/conflict-helper/diagnostic formatting. Dependency input is the public resolved-command data, not a reimplementation of alias resolution. Cases: nonbuiltin, unchanged invocation, aliased duplicates with a lone-surrogate source path, and mixed/case-sensitive names.

[Branch/proof map](../porting/traceability/interactive-mode.md#builtin-conflict-warnings).
