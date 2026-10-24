# Ordinary-object prototype lookup gaps

[Policy](../../specs/object-prototype-lookups.md): approved runtime-oddity exclusion, 2026-10-23. Retired `pi-js-compat::object_prototype` and its exclusive tests. No frozen TS module corresponds to that former runtime-emulation table.

| Frozen boundary | Native behavior | Status/proof |
|---|---|---|
| interactive-mode.ts:391–403 display-name inherited truthiness | Explicit display-map keys, then built-in/OAuth exclusions | gap; own_key_tables native regression |
| interactive-mode.ts:6879–6898 inherited default interpolation | Explicit provider defaults only | gap; ordinary completion frame/error proofs retained |
| idle-compaction.ts:29–33 inherited TTL yields NaN | Unsupported API returns None | gap; own_key_tables regression; explicit API differential retained |
| theme.ts:1156–1221 inherited extension value | Unknown extension returns None, renders unhighlighted | gap; own_key_tables regression; named-language/grammar proofs retained |

Historical inherited-value matrices are retired, not current parity evidence. Existing callback error carriers and outer render catch behavior remain unchanged; only the emulated prototype-generated failures are removed.
