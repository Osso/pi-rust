# Ordinary-object prototype lookups

Native `pi-js-compat/src/object_prototype.rs`; [contract](../../specs/object-prototype-lookups.md). Runtime intrinsic consumed by common-pin plain-object tables, not a separate frozen TS module.

| Boundary | Mapping | Proof |
|---|---|---|
| Object.getOwnPropertyNames(Object.prototype), ordinary-object lookup | One name/String(value) table | object_prototype::inherited_ordinary_object_values_match_pinned_node checks all12 names, text, runtime type, truthiness and numeric NaN; absent/case-sensitive keys |
| idle-compaction inherited TTL | Shared membership, numeric NaN | Existing96-case actual-source idle matrix; local table deleted |
| isApiKeyLoginProvider inherited display-map truthiness | Shared membership before built-in/OAuth exclusion | Real models.json+OAuth provider cases; constructor RED before fix |
| authentication default-model lookup | Shared inherited text for error interpolation only; functions/objects never match string IDs | Real models.json/capture-UI completion matrix: constructor RED q15-auth-completion-inherited-red.log; GREEN scope recorded in lane ledger |

Generated oracle data: Nodev26.11.1/V8 14.6.202.34-node.37. `__proto__` was read on an ordinary object, correctly yielding `[object Object]`. The String projection does not represent the original runtime object's type or identity; consumers must preserve strict inequality against string IDs. Source discrepancies are retained literally, not fixed into own-property-only lookups. Mutation of Object.prototype is not modeled; no frozen built-in consumer mutates it.
