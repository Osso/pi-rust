# Package-manager semver behavior

Package-manager must match the frozen `semver@7.8.0` calls, with strict/default options:

- Source parsing retains the original npm spec/version and computes `range`/`pinned` exactly as `validRange`/`valid`, including JS whitespace, prefixes, build/prerelease grammar, wildcard/caret/tilde/hyphen/comparator/OR forms and invalid boundaries.
- Installed-version checks and maximum selection match `satisfies`/`maxSatisfying`, including prerelease admission, invalid candidates and first-item ties.
- Unranged version arrays use stable descending `rcompare` order. Build metadata does not break ties. Numeric prerelease comparisons retain JS binary64 semantics. Invalid singleton strings remain selectable because sorting invokes no comparator; multi-element invalid values retain native TypeError behavior.
- UTF-16 source strings remain exact. Invalid grammar does not authorize replacement of unpaired units. Version constructors enforce the original pre-trim256-unit limit and safe-integer core bounds.

Only dependency APIs actually called by package-manager are in scope. Invalid JS runtime input types remain excluded by the typed API contract. This work does not establish process-output capture, timeout or environment parity.

[Source mapping and proof](../porting/traceability/package-manager-semver.md).
