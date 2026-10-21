# JS-value boundary proof

Scope: `/tmp/pi-rust-serialization-audit/REPORT.md`, base `b5cfa925`.
Decision: [JS JSON spec](../specs/js-json.md#js-visible-serde-boundaries).
Commands, revisions and invalidation: `/tmp/pi-rust-js-value-semantics/ledger.md`.
No parser, lossless representation, session codec or generic serde behavior changes.

## Coverage

| Boundary | Concrete behavioral proof | RED |
|---|---|---|
| Agent tool-details cap and preview | `agent_loop::js_details_at_storage_boundary_are_not_truncated`; `differential_agent_loop` numeric keys/leaves in oversized details | `03-red-agent.log` |
| Approval and allow-always prompts | `agent_session::human_allow_always_spawns_the_permission_config_agent`, exact pretty input in both outputs | `09-red-approval.log` |
| Unknown-tool tree label and truncation | `differential_interactive_components` case 169; private `unknown_tool_label_stringifies_before_truncating` | `07-red-tree.log`, `08-red-tree-diff.log` |
| Pyrun result and progress text | `pyrun_extension` result/progress formatting, raw `{"2":2.0,"1":1.0}` | `06-red-boundaries.log` |
| Remote replacement bytes, dedup, argument string | `differential_openai_remote_compact`: numeric response history, two otherwise identical messages with 2.0/2, tool args | `06-red-boundaries.log` |
| Remote error fallback and serialized character budget | private `error_fallback_uses_javascript_json`, `serialized_budget_uses_javascript_numbers` | `05-red-lib.log` |
| Export HTML/base64 payload | `differential_export_html`: temp session with arbitrary numeric details, all themes compared byte-for-byte | `06-red-boundaries.log` |
| Session-entry persistence | `session_tool_output::persisted_details_use_javascript_key_order_and_numbers`; cap accounting uses same bytes as writing | `06-red-boundaries.log` |
| History object traversal | private `collects_numeric_properties_in_javascript_order` | `05-red-lib.log` |

The primitive compact/pretty/number/order mutations remain covered by the existing JS JSON differential targets. Boundary regression tests are run against the pre-fix code first; no implementation-shape assertions.

## Uncertain groups resolved

| Candidate | Resolution and source evidence | Behavioral proof |
|---|---|---|
| Search scanning nested numbers | CONFIRMED: `EntryBody::Custom.data` is arbitrary `OptionalValue`; default search serializes the whole entry | `session_search::searches_javascript_json_for_nested_values`, RED `03-red-agent.log` |
| History pretty details | CONFIRMED: `SessionHistorySearchEntry.content` includes neighboring assistant tool-call arguments | `search_current_session_history_tool::context_tool_arguments_use_javascript_json_in_visible_text`, RED `06-red-boundaries.log` |
| Detached bridge | CONFIRMED: pinned `extensions/pyrun/src/detached-bridge.ts:129-142` accepts `body: unknown` and `JSON.stringify(input.body)`; request params/response result arbitrary | `detached_pyrun_bridge` verifies durable request body bytes before parsing, RED `06-red-boundaries.log` |
| Remote dedup numbers | CONFIRMED: message object minus id retains arbitrary nested numeric fields; 2 and 2.0 must deduplicate | Pinned remote differential numeric-history-dedup, RED `06-red-boundaries.log` |
| SDK settings | CONFIRMED: `Settings = Map<String, Value>`; arbitrary nested custom settings survive loading | Pinned `example_sdk_settings` custom numeric keys, RED `06-red-boundaries.log` |
| Model config IDs | CONFIRMED: pinned `core/model-config.ts:280-292` builds Map from `Object.entries(config.providers)`; Rust skipped that enumeration | Pinned `differential_model_config` numeric-provider-order, RED `06-red-boundaries.log` |

## Additional pattern inventory

Initial inventory: `/tmp/pi-rust-js-value-semantics/pattern-inventory.txt`.

- `session_tool_output::serialized_entry_bytes`: additional same-family cap measurement fixed alongside persistence, so cap and writer agree.
- `session_tool_output::serialized_content_bytes`: text/image records only; no arbitrary numeric/key-bearing leaves. Unchanged.
- Remote `count_reducible_text_chars` sums every child: traversal order cannot affect the integer sum. Unchanged.
- Agent-session tool registries are JS Map equivalents, not plain-object enumeration. Unchanged.
- SDK skills diagnostics and model-name arrays: typed string-bearing records; no demonstrated numeric/object divergence. Unchanged.
- Additional confirmed sites: Pyrun request/reply wire bytes (`pyrun_extension::runner_requests_and_bridge_replies_use_javascript_json`), launch manifest bytes/checksum (`differential_pyrun_detached`), remote HTTP request bytes/error response (private loopback `http_body_uses_javascript_json`), and shared artifact hash/JSONL (`js_value_artifacts`). RED logs 14–17. Shared manifest checksum failed its own read-back with numeric leaves before the fix.
- Pyrun restore comparison: persisted JS-equivalent params must not report a collision; filesystem regression first failed on manifest checksum (14), then on the false collision after repairing the manifest (18). Restore now compares shared JS JSON bytes while retaining the existing collision error.
- Detached activation identities and Bash atomic launch manifests contain fixed string/integer fields, not arbitrary JSON leaves; unchanged.

## Proof status

RED complete for the rows above. GREEN, mutation and preservation results are recorded after implementation. Unfiltered library RED was aborted for unrelated hanging UI tests; its partial run is not acceptance evidence. Subsequent library runs use `js_value_tests` filters. No check, clippy, workspace suite or delegation.

Pyrun formatting also has a pinned eval-module differential (`differential_js_value_pyrun`), covering numeric key order, nested binary64 values, -0, exponents and unsafe integers. Native remote tests isolate argument encoding and numeric dedup so an earlier failed differential case cannot hide their mutation sensitivity.
