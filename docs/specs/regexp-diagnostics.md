# Shared RegExp diagnostics

[Approved contract/proof](../porting/traceability/regexp-diagnostics.md); user decision recorded in AGENTS.md, 2026-10-22.

- Preserve valid/invalid classification against pinned Node. Reject duplicate/unknown flags and mutually exclusive u+v flags. Use Unicode input mode for both u and v.
- Expose raw UTF-16 `Invalid regular expression: /<source>/<flags>: <reason>` messages through one JsRegex authority. Invalid constructor flags have the Node constructor-flags prefix. Display encoding occurs only at native formatting/output boundaries.
- Retain existing, pinned-oracle-proved V8 reason mappings from bwrap in that shared authority. Other reason wording may differ under explicit approval; do not duplicate scanners/builders in consumers.
- Record regress depth/loop limits as explicit accepted-by-Node/rejected-by-native gaps (257 nested groups;65,536 quantified atoms), not parity or ignored tests.
