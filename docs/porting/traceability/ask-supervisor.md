# Ask supervisor — common-pin map

Entire `packages/coding-agent/src/core/tools/ask-supervisor.ts` at immutable `4f9e6f8a4`; native `core/tools/ask_supervisor.rs`. [Contract](../../specs/ask-supervisor.md). Shared [client](supervisor-client.md), project resolver, resident transport and control DB implementations are unchanged.

## Symbol and branch coverage

| Frozen symbol/branch | Native | Proof |
|---|---|---|
| advisory timeout180000ms; schema question/context4000/8000; input/details; factory metadata/prompts/sequential mode | Exact constants/schema/ToolDefinition and native JsString fields | Full descriptor differential; real posted deadline must lie within call-start/end plus180000ms; full4000/8000 payload history |
| execute noSupervisor before child/path/sender checks | Live context bool getter, then existing helpers | Five safe early-guard source/native histories, including standalone+child precedence; no project/service access |
| assertMainSession: nonempty agent ID OR required flag | Nonempty native filter, not Option presence | Actual empty-agent RED before7ccaa4a1; child/required/missing-path controls GREEN |
| requireControlDbPath: absent or empty failure | Native optional nonempty filter | Missing-path control plus real loaded-empty-ID guard-order history; actual empty-path RED before correction |
| senderSessionId guard | Real SessionManager.get_session_id, empty rejection | Both frozen and native load an empty ID from a valid persisted header; valid-path→sender error and empty-path→path error. No invariant exclusion or SessionManager edit |
| resolveSupervisorProjectForCwd(ctx.cwd, default KB) | Existing resolver and literal default root | Actual source/native project mapping through a private bind mounted at `/syncthing/Sync/KB`; live KB never exposed to the probe |
| question plus context only when defined; durable advisory kind, project/sender IDs, timeout | Existing advisory_payload/request_supervisor_decision, no injected production seam | Five actual request/response histories compare parsed payload, exact payload JSON, kind, IDs, deadline and full tool outcome; absent/empty context, empty question and raw units covered |
| ignored _signal/_onUpdate | Native tool does not pass signal/update to request client | Pre-aborted supplied-reason case still posts/completes a durable request; source/native updates remain empty |
| advisory response→answer text/details; no terminate/isError | text_result with original answer/project/sender | Raw-D800 answer and empty-answer histories, complete result comparison |
| error response→reason rejection | Native error reason directly retained | Full raw-D800 reason source/native history |
| other response kind→invalid-advisory rejection | Existing wildcard response branch | Validly encoded Approve stored as legacy/corrupt completed-row fixture; source/native exact error. Normal completion API remains unchanged and correctly rejects that mismatched kind |
| renderCall/renderResult Text reuse/new/setText; joined text-only result/error/empty output | Existing fallible set_text_component and render_tool_output | Exact descriptor plus14-frame history: raw units, mixed text/image, empty/error results, narrow width, reuse/reset and incompatible-component TypeErrors |

The persisted-ID wording does not add a session-file requirement: source checks only the ID string, and an actual in-memory manager with a valid ID is accepted. Invalid JavaScript runtime argument field types remain outside the recorded typed API; no source-unrequested trim, clipping, fallback or validation was added.

## Empty ID classification

Frozen `newSession` validates caller IDs at session-manager.ts:1392-1395 using265-270. That is not a global manager invariant: setSessionFile:1376 assigns header.id verbatim, open:2242-2251 uses that path, and openReadOnly:2264 also assigns it. Native loaded assignment at session_manager.rs:2854 behaves the same. Real file-backed differential proves the empty sender guard remains reachable on both sides. The first in-memory-empty-ID fixture was invalid setup, not parity evidence; no typed-context exclusion was taken.

## Isolated durable proof

Main approved the test-only namespace/socket/DB fixture2026-10-24. The outer test starts a nested bwrap namespace, hides `/syncthing`, mounts a private KB at the exact frozen root, and binds only its private test state writable. The inner test verifies the mount before importing/running the tool. Private HOME/XDG/agent/tmp roots are supplied; the existing outer Cargo guard keeps networking loopback-only.

An identity-only resident console uses existing transport APIs with the current version and external-management identity. Both native and frozen clients preflight that socket/version before requests; no Supervisor agent/provider/service is launched. A real private control DB responder claims and completes requests. Databases are initialized before advertising the ready identity, so this tool proof does not test concurrent first-open schema migration.

Five histories cover raw question/answer, absent versus empty context, empty question/answer, pre-aborted ignored signal, complete4000/8000-unit payloads, raw error reason and unexpected persisted response kind. Exact request payload JSON and source/native results are compared; deadline interval checks establish the180000ms argument without sleeping three minutes. Startup/retry/timeout internals remain the shared client's own scope. Test helpers add no production dependency injection or validation bypass.

## Proof ledger and setup corrections

- `ask-supervisor-guards-red.log`: invalid inMemory-empty-ID fixture, not behavioral RED. `ask-supervisor-guards-red-2.log` is valid RED2 (empty agent and control path).7ccaa4a1 GREEN2 tests/7 histories: `ask-supervisor-guards-green.log`.
- `ask-supervisor-render-red.log`: descriptor GREEN, real retained-component replacement RED.00ba5c38 routes existing fallible Text helper; descriptor+14frames GREEN `ask-supervisor-render-green.log`.
- Private fixture setup probes are not production parity failures: missing KB mountpoint, inherited nodev devices, raw JsObject serialization and intentionally mismatched completion through the validating API were corrected in test code. Main confirmed explicit JsValue::Object lossless carrier and test-only persisted SQL response injection. Phase logging localized the failures; bounded fixture shutdown left no surviving processes.
- One probe raced native/frozen first-open migrations and hit `duplicate column origin_host`. The fixture had advertised a ready resident before initializing its DB. Initialization is now ordered before socket readiness; no shared migration code was changed or claimed fixed.
- `ask-supervisor-roundtrip-final.log` GREEN outer behavioral test, executing all five source/native histories inside the ignored private-namespace helper. The helper is not an omitted feature/gap test. e49ae8b5 commits this proof fixture; later helper naming/formatting is behavior-neutral.
- Final behavior revision `b1aed8e1` (parent `9ac6affc` merged): `cargo test -j4 -p pi-coding-agent --test differential_ask_supervisor` passes3 outer tests (7 guard histories and5 durable histories); the ignored helper is explicitly executed inside the isolated outer test. `differential_change_cwd_renderers ask_supervisor` passes descriptor/history2tests,14frames. Logs: `ask-supervisor-final-{runtime,render}.log`.
- Same revision: workspace all-target keep-going JSON check has0errors/0warnings; warning-mode all-target/all-feature coding-agent Clippy exits0,1686 baseline messages and0 owned changed-primary-span findings against `ef3d3d1c`. Strict protected frozen-reference TS compilation passes all3 changed fixtures. Biome retains23 pre-existing warnings/1 formatter error in the general oracle,0 changed-line lint/format findings; advisory/render fixtures are clean. Rust helper phases were named during manual Rust/TS readability audit; no new suppressions/dependencies/fallbacks.
- Exact commands/revisions and logs: `/tmp/pi-rust-repin-core-tools/ledger.md` and `ask-supervisor-{workspace,clippy,biome}-summary.json`, `ask-supervisor-types.log`, `ask-supervisor-inventory.log`. Inventory PASS:686rows/186679lines;451done/19gap/48partial/168todo. Final checkpoint edits documentation only. No live KB/provider/service, shared sandbox, runtime, storage or DB implementation edits.
