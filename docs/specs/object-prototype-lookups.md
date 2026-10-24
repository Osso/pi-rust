# Plain-object lookup policy

Approved 2026-10-23: Node/V8 prototype leaks and native-function stringification are excluded. Native plain-object tables use explicit own keys. No shared prototype-emulation authority remains.

- Login eligibility uses the display-name table before built-in/OAuth exclusions.
- Default-model selection uses only explicitly configured provider defaults.
- Idle compaction supports only the two explicit API TTL entries.
- Filename extensions resolve only to named languages; other extensions render unhighlighted.

Frozen application branches, console output, UTF-16 text and error carriers remain exact outside these excluded runtime cases. Module gap rows and surviving proof scopes: [traceability](../porting/traceability/object-prototype.md).
