# JS-value boundary proof

Base: `b5cfa925`. Decision: [JS JSON spec](../specs/js-json.md#js-visible-serde-boundaries).
Detailed commands/revisions: `/tmp/pi-rust-js-value-semantics/ledger.md`.
Full report and target inventory: `REPORT.md`, `proof-targets.json` in that directory.
No parser, lossless representation or session codec changes. No merge/push, delegation, Cargo check, Clippy or workspace suite.

## Per-boundary coverage

Log numbers below name `/tmp/pi-rust-js-value-semantics/NN-*.log`.
Every row has concrete output/state assertions, not source-shape assertions.

| Boundary | Behavioral proof | RED | GREEN | Mutation |
|---|---|---|---|---|---|
| Agent details cap/preview | `agent_loop` exact 10240-byte JS boundary; pinned `differential_agent_loop` numeric keys/leaves in oversized details | 03 | 10 | 21 |
| Approval and allow-always prompts | `agent_session::human_allow_always_spawns_the_permission_config_agent`, exact pretty input in both outputs | 09 | 29 | 24 |
| Unknown-tool label/truncation | Pinned interactive case 169 includes actual tool-result row; private short-label test | 07–08 | 11, 33 | 22–23 |
| Pyrun result/progress text | Native formatters; pinned `differential_js_value_pyrun` exercises the real eval module | 06 | 20, 34 | 23 |
| Remote replacement-history byte count | Pinned `differential_openai_remote_compact`, nested 2.0/1.0 | 06 | 13 | 23 |
| Remote error fallback | Private exact JSON fallback and real loopback HTTP 400 test | 05 | 33 | 22 |
| Remote tool-call arguments | Pinned payload comparison plus isolated native argument string | 06, 22 | 13, 33 | 22 |
| Remote serialized character budget | Private `serialized_budget_uses_javascript_numbers` asserts 15 UTF-16 units | 05 | 33 | 22 |
| HTML/base64 session payload | Pinned `differential_export_html`, arbitrary numeric details/arguments; complete HTML bytes for all themes | 06 | 13 | 23 |
| Session-entry persistence | `session_tool_output::persisted_details_use_javascript_key_order_and_numbers`; cap accounting uses the same writer | 06 | 11 | 23 |
| History object traversal | Private `collects_numeric_properties_in_javascript_order`, `first, second` | 05 | 33 | 22 |

Mutation 21–24 replaces shared compact/pretty writers with raw serde and object enumeration with insertion iteration. All affected boundary regressions fail; original bytes were restored before preservation runs. The pre-fix RED additionally proves the boundary adopts the helper, rather than merely testing helper behavior.

## Uncertain groups resolved

The report contains six uncertain groups, despite the brief's count of five. All are CONFIRMED; no group was declared safe without evidence.

| Candidate | Reachability/source evidence | Proof: RED → GREEN; mutation |
|---|---|---|
| Search scanning nested numbers | `EntryBody::Custom.data` is arbitrary `OptionalValue`; default search serializes the entry | `session_search::searches_javascript_json_for_nested_values`: 03 → 10; 21 |
| History pretty details | Context entries include assistant `content` with arbitrary tool-call arguments | Native context search and existing pinned history-tool differential: 06 → 37–38; 23 |
| Detached bridge | Pinned `extensions/pyrun/src/detached-bridge.ts:129-142` takes `body: unknown` and stringifies it; params/results arbitrary | Durable request-body assertion before parsing: 06 → 11; 23 |
| Remote dedup numbers | Message object minus id retains nested numeric leaves; 2.0/2 must deduplicate | Pinned numeric-history case plus isolated latest-message test: 06 → 13, 33; 22 |
| SDK settings | Rust `Settings = Map<String, Value>`; arbitrary custom settings survive disk loading | Pinned settings stdout with numeric custom keys: 06 → 11; 23 |
| Model config IDs | Pinned `core/model-config.ts:280-292` constructs its Map from `Object.entries(config.providers)` | Pinned `numeric-provider-order`: 06 → 11; 23 |

## Additional pattern fixes

| Boundary | Proof |
|---|---|
| Pyrun request/reply wire bytes | Python subprocess captures exact input/reply lines: RED 15, GREEN 20, mutation 23 |
| Pyrun manifest bytes/checksum | Pinned `differential_pyrun_detached`: RED 15, GREEN 20. Numeric manifests previously failed their own read-back (14) |
| Persisted Pyrun params equality | Real manifest write/read and terminal artifact restore: RED checksum failure 14, then isolated false collision 18; GREEN 33. Existing collision error retained |
| Remote HTTP request body | Loopback request capture plus error response: RED 14, GREEN 33, mutation 22 |
| Session branch JSONL export | Public `AgentSession::export_to_jsonl` persists custom data with numeric keys and 9007199254740993; pinned source lines 5999/6005 use stringify. RED 43; GREEN pending |
| Shared artifact hash/JSONL | Concrete bytes/digest in `js_value_artifacts`; manifest differential checks shared hash integration: RED 15/17, GREEN 20; mutation 23/22 |

## Remaining inventory

Exact before/after grep: `pattern-inventory.txt`, `remaining-inventory.txt` in the evidence directory.

- `session_tool_output::serialized_content_bytes`: typed text/image records; no arbitrary numeric/key-bearing leaves. Unchanged.
- Remote `count_reducible_text_chars`: sums all child lengths; object visitation order cannot change the sum. Unchanged.
- Agent-session tool registries are JS Map equivalents. URLSearchParams keys in the untouched HTML template are not JSON object keys. Unchanged.
- Detached activation identities have fixed string/integer fields. Final inspection found Bash launch manifests also contain an environment-name map: keys 1/2/10 expose lexicographic-versus-JS order. RED 39; the shared atomic writer now uses JS stringify (GREEN 40). No raw arbitrary-Value stringification remains at the audited boundaries. This is not a whole-repository serde migration or proof for arbitrary native unsafe integers.
- SDK skills diagnostics/model-name arrays are typed string-bearing records/arrays; the report established no numeric-key or numeric-leaf counterexample. Unchanged.

Typed-schema property insertion order remains a separate existing caveat: adding `details` after `timestamp` in a caller-built file differs from Rust's schema order. The HTML fixture changes existing details fields, isolating this task's numeric-key/number defect rather than claiming that caveat fixed. Remote differential compares complete JS-visible bytes, not serde's internal integer/float variants; no fields are dropped.

## Preservation and limits

57 executed targets, 551 passing test functions across the retained scoped logs. Six ignored entrypoints are child-process probes exercised through their parent tests. `proof-targets.json` maps exact targets/counts/logs; it combines the ten passing session-tool cases from 31 with the resume case in 32. The latter required only the documented local v15 reference setup, not a product change.

| Proof scope | Logs |
|---|---|
| Agent loop and search | 10 |
| Session codec/JSONL, unsafe-number boundaries, byte comparisons and conformance | 25 |
| Core compaction targets | 26 |
| Shared lossless JS JSON and legacy JS JSON | 27–28 |
| Coding compaction, export, remote extension and persistence constants | 28 |
| Full agent-session target/family | 29–30 |
| Session-manager, tree-selector native coverage, continuation and session tools | 31–32, 38 |
| Pyrun durable/process/native targets, artifact hashes and formatting | 20, 33–34 |
| Final history conversion reuse | 37–38; other module proofs unchanged |

**Existing real-Pyrun differential is blocked, not proven.** Target `differential_pyrun` exits successfully by skipping its test. Instrumentation (35–36) established that installed standalone Python fails `_Py_HashRandomization_Init` inside the existing sandbox; the test misreports this as uninstalled. No sandbox/device changes or hash-seed workaround were introduced. Real subprocess wire tests and the pinned eval-module differential execute, but do not substitute for that external-runtime proof.

The initial unfiltered library RED entered unrelated hanging UI tests and was stopped; it is not acceptance evidence. Later unit runs select only `js_value_tests`. Manual Rust/TS changed-line review found no new suppressions, fallback paths, unreachable code or production control-flow complexity; formatter and final repository state are recorded separately. No independent-agent verification was performed because delegation was forbidden.
