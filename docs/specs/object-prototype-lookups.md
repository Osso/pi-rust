# Frozen ordinary-object inherited lookups

`pi-js-compat::object_prototype` is the single authority for inherited property names and String(value) projections used by frozen plain-object lookup consumers. Values are derived by `interactive-login-oracle.mts` from the common-pin Node process, not invented function bodies. Current oracle runtime: Node26.11.1/V8 14.6.202.34-node.37.

- Enumerate Object.prototype's own names, then look each name up on an ordinary object. In particular, ordinaryObject.__proto__ is Object.prototype, not null.
- All twelve values are truthy, are not strings, and coerce numerically to NaN. Missing names remain absent.
- String projections are interpolation-only. A model's string ID must never compare equal to an inherited function/object merely because its text matches that projection.
- Idle compaction retains the classified inherited-TTL NaN discrepancy; API-key eligibility retains inherited display-table truthiness. Authentication default-model interpolation must use the same authority.
- This models the unmodified built-in prototype used by frozen consumers, not a general JavaScript prototype/mutation runtime.

Proof: shared table differential checks every name/projection/type/truthiness/coercion against the pinned oracle. Real models.json provider-ID cases exercise login option assembly with registered OAuth names; idle and authentication consumers have their own source differentials. Completion/count consumer integration remains separately tracked until its proof lands.
