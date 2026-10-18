# permission rule store traceability

Pinned source: `packages/coding-agent/src/core/permissions/rule-store.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/core/rule_store.rs`.

Differential proof: `oracle/permission-rule-store-oracle.ts` drives `buildPermissionRuleContent`,
`writePermissionRules`, and `PermissionRuleStore` against the pinned module; the Rust side repeats
every write on the same fixture and compares the resulting file bytes, the writer callbacks, and
error messages (`tests/differential_config_auth_state.rs`): 5 content cases, 10 fixtures × 3
destinations, and a 9-step store sequence. Focused tests: `tests/permission_rule_store.rs` (9 cases).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `rule-store.ts:5-30` | Destination/behavior/update/options shapes; writer callback | `PermissionRuleDestination`, `PermissionRuleBehavior`, `PermissionRuleUpdate`, `WritePermissionRulesOptions`, `PermissionRuleWriter` |
| `rule-store.ts:44-89` | Store: exact allow-rule matching, settings loading, session updates vs. persisted updates | `PermissionRuleStore`; focused + differential store sequence |
| `rule-store.ts:91-97` | Rule content: bash command string, otherwise compact JSON | `build_permission_rule_content`; focused + 5 differential cases |
| `rule-store.ts:99-122` | Write: merge existing rules, dedupe, keep other settings | `write_permission_rules`; focused + differential fixtures |
| `rule-store.ts:124-133` | Destination paths (`settings.json`, `.pi/settings.json`, `.pi/settings.local.json`) | `resolve_settings_path`; focused per destination + differential |
| `rule-store.ts:135-142` | Non-object settings parse as `{}` | `read_settings`; differential `no-closing-brace`/invalid cases |
| `rule-store.ts:144-173` | In-place splice: replace a top-level `permissionRules` value, otherwise append before the final brace with the compact/pretty rules | `update_permission_rules_content`; differential fixtures (compact, pretty, empty, spaced, null, nested string, no brace, unicode) |
| `rule-store.ts:175-210` | Top-level property scan with string awareness and JSON-decoded names | `find_top_level_property_value_range`; differential fixtures |
| `rule-store.ts:212-232` | Value end scan (objects/arrays/strings/commas) | `find_json_value_end`; differential fixtures |
| `rule-store.ts:246-274` | Permission-rule normalization (only string arrays under `allow`) | `read_permission_rules`; focused invalid-shape test + differential |

## Gaps

- None.

## Discrepancies

- None observed. The `nbsp-before-value` fixture is invalid JSON for both runtimes; both report the
  same V8 `SyntaxError` message.

## Proof inventory

- Focused cases: 9.
- Differential: 5 content cases, 30 write cases (10 fixtures × 3 destinations), 9 store steps.
