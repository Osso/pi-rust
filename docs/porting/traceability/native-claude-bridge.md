# Native Claude bridge audit

Source: read-only extension `38d84cb549cce66dd76d45a5020d132f74780393`, `src/`.
Native directory: `crates/pi-coding-agent-extensions/src/native_user_extensions/claude_bridge/`.
Audit baseline: `c4bc63836`. This is an implementation/proof inventory, **not parity closure**. The table and numbered findings below retain that historical baseline; implemented follow-ups are recorded here and in the [current spec status](../../specs/claude-bridge-native.md).

## Cancellation follow-up

- Provider protocol operations (initialize, prompt/steer/control writes, reads and close) are under one abort boundary. Session shutdown/discard has a separate reason-bearing wakeup, so it also interrupts blocked writes. Offline tests fill the tools/list reply pipe, then abort or shut down and assert stream settlement plus child reaping.
- AskClaude and isolated summaries retain child ownership outside their cancellable protocol futures and await reaping on cancellation/errors. The source AskClaude catch stops its interval but does not blank progress; native preserves that behavior. Tests stall initialization and return an error while the CLI stays alive.
- Quota rechecks check cancellation after the response and do not publish new account evidence from a cancelled probe. Provider output uses Aborted, not Error, and retains the cancellation error text. The injected HTTP test compares account state before and after that boundary.
- Parked MCP calls drain in insertion order with plain-text reason replies on abort, shutdown, discard and normal/error end. Queued results are cleared. The source resolves promises before close without waiting for transport delivery; native attempts the equivalent writes without allowing a blocked pipe to delay reaping. Exact reply-value/queue tests complement the fake-process lifetime tests. Full bidirectional MCP proof remains open.

The source's TypeScript loader is not executed at runtime. Its SDK/MCP/cc-session-io contracts are implemented natively.

## Source coverage

| Source module | Native representation and evidence | Remaining work |
|---|---|---|
| `account-profiles.ts` | `account_profiles.rs`, `account_state.rs`; migration, canonical refresh writes, denial/recovery merge and frozen TS bidirectional state-file test | Authentication tracing; credential freshness tie cases need dedicated native proof |
| `account-recheck.ts` | `usage.rs::UsageService::recheck`, `provider.rs::prepare_account_launch`; offline capacity/retry tests | Cancellation while a usage HTTP call or refresh process is pending |
| `agents-md.ts` | `prompt_capture.rs::project_context`; exact context/rules projection tests | None found in formatter |
| `askclaude-schema.ts` | `ask_claude.rs::AskDefaults`, `ask_render.rs`; schema/default/render tests | JSON null defaultMode is not treated like missing; schema fixture proof is not exhaustive |
| `askclaude-ui.ts` | `ask_claude.rs::format_action`, `action_summary`; native action tests | Action verb splits only on space, not source whitespace regexp |
| `attachments.ts` | `native_prefix.rs::tail_attachments`; attachment-chain and persisted-record tests | Warning surface is stderr rather than source debug log |
| `auth-trace.ts` | No native trace writer | Full private trace format, metadata fingerprints, deduplication and stderr classification |
| `claude-usage.ts` | `usage.rs`; synchronous event claims, sequential accounts, bounded HTTP retry and idle-refresh fake-process tests | Request-context notification/sendMessage end-to-end proof and cancellation propagation |
| `config.ts` | `config.rs`; merge, notice persistence and malformed-config preservation tests | First-query notice integration; precise parser/I/O diagnostic wording is not proven |
| `convert.ts` | `convert.rs`; frozen message conversion fixtures, parallel-result hoist and image tests | Dropped-content debug accounting |
| `extract-tool-results.ts` | `provider.rs::recent_tool_result_ids`, `accept_results`; tool/steer subprocess tests | Empty/unsupported result-content normalization differs from source |
| `index.ts` | `mod.rs`, `provider.rs`, `process.rs`, `sessions.rs`, `summaries.rs`, `lifecycle.rs`; 53 offline bridge tests plus real-binary startup | See open requirements below |
| `log-paths.ts` | No native log-path authority | Debug override and dynamic agent-dir paths |
| `mcp-server.ts` | `provider.rs::control`, `mcp_result`, `send_mcp_response`; schema and tool-use-ID round-trip fixture | Bidirectional full MCP handshake proof is incomplete; cancellation kills peer instead of source's final handler replies |
| `models.ts` | `models.rs`; catalog/order/context/plan tests and real offline model listing | Model-specific effort/null suppression is covered by SDK subprocess tests; exact source name-word-boundary/locale ordering not proven |
| `native-prefix.ts` | `native_prefix.rs`, `session_records.rs`, `sessions.rs`; frozen JSONL writer/readback and association tests | Source best-effort prior/association read errors differ from native explicit propagation; account-selected session directory mismatch |
| `pi-cwd-api.d.ts` | Native extension cwd and stream cwd fields | Declaration-only; no runtime code |
| `prompt-capture.ts` | `prompt_capture.rs`; exact capture, inheritance, UTF-16, explicit unresolved error and approved user_rules tests | Diagnose/debug callback wiring; exhaustive rejection-error text fixture proof |
| `prompt-stream.ts` | Native actor/channel and awaited process stdin writes; FIFO multi-steer test | Writes/control replies can block cancellation; failed-steer best-effort delivery/rebuild differs |
| `query-state.ts` | `provider.rs::QueryActor`, `account_state.rs`; concurrent identical tool IDs, shutdown, rewrite and denial tests | Cross-instance serving ownership; rate-limit warning state; cancellation-safe settlement |
| `session-verify.ts` | No native post-write verification | Warning calculation, UI warning and diagnostic dump |
| `shape-trace.ts` | No native shape trace writer | Private capped JSONL format, phases, ID filtering and I/O warnings |
| `skills.ts` | `prompt_capture.rs::render_skills`; native/MCP/no-reader projection tests | None found in formatter |
| `transcript.ts` | Typed AI Context is already the legacy system_prompt/tools/messages shape used by this port | No direct frozen section-replay proof; upstream transcript-shaped ingress must be checked at its shared runtime boundary |
| `usage.ts` | `stream.rs::update_usage`; partial-frame/reasoning counters tested | Cost calculation for caller-supplied nonzero-cost models is omitted; registered bridge models have zero rates |

## Open requirements at baseline

These are missing behavior or proof, not approved deviations.

1. **AskClaude shared history/resume and cancellation.** Source `index.ts:2666–2861` imports `buildSessionContext` history, reuses the calling Pi session's native mirror, and applies isolated persistence. Native `ask_claude.rs:273–361` starts with empty context and no resume. Its initialize wait and close-after-interrupt are outside cancellation; the isolated summary path is separate and does not repair this. Source completion clears progress; native only clears it on success.
2. **Provider ownership across extension instances.** Source `index.ts:508–562, 3103–3150` keeps the first serving stream, skips registration when the child registry already has the provider, registers into an independent child registry and forwards rewrite marks to every serving instance. Native `mod.rs::activate` always registers a newly allocated `BridgeProvider`; only prompt captures are process-shared.
3. **Rate-limit notifications and error labels.** Source `index.ts:790–794, 1880–1925` emits rejection/warning notifications (5% deduplication, threshold change, allowed rearm) and labels failed output. Native `provider.rs::settle_account_frame` only changes selection/exhaustion. It updates account evidence before noncancelled settlement, unlike source `settleQueryAccount`. Source retry-permission clearing on nonretryable/context-overflow errors is not yet matched.
4. **First TUI query notice and post-write checks.** Source `index.ts:1359–1371` defers the notice and config date to the first TUI bridge call; native `config.rs::write_startup_notice` has no production caller. Source `session-verify.ts` and `index.ts:1038–1056` warn about saved-file integrity; native `sessions.rs::sync` writes without that check.
5. **Auth, shape, debug and diagnostic logs.** Source `auth-trace.ts`, `shape-trace.ts`, `log-paths.ts`, and `index.ts:74–125, 1849–1857` write private traces, optional full debug/CLI stderr logs, unconditional diagnostics and optional raw stream capture. Native has no corresponding writers. Source post-write failures warn and continue; native does not expose the warning.

Additional implementation/proof issues found during the same audit:

- **Compaction takeover is pending user decision.** Source `index.ts:3006–3039` returns `compaction` from `session_before_compact`; the frozen native result permits only `cancel` there. Separate native `compaction` hook adaptation is not registered. Draft helper/tests are saved outside the checkout in `/tmp/claude-bridge-item5-checkpoint`.
- **Profile transcript directory.** Native `prepare_launch` writes/imports through `SessionStore.claude_dir` from ambient `CLAUDE_CONFIG_DIR`, but launches a named profile under its selected private directory. A history rebuild may therefore be unreadable to that child. Source session IO uses its ambient directory too; classify the source behavior before changing semantics, rather than invent a new format.
- **Cancellation coverage and cleanup.** Provider initialize, initial write and top-level reads are tested. Tool/control writes, account rechecks, AskClaude initialization/close, summary shutdown and malformed-peer/error exits lack complete cancellation/peer-reaping proof. Query cancellation currently drops queued RPC handlers and kills the child rather than sending source-equivalent final replies. Source `index.ts:2017–2030` preserves tool-result delivery after a missed steer and marks a rebuild; native returns an error from that write.
- **Provider metadata/SDK launch (resolved slice).** Native `models::effort` honors source `index.ts:2434–2446` thinkingLevelMap precedence, undefined fallback and null/invalid suppression. Provider hooks initialize even with no MCP tools. Nine installed-SDK captures prove exact argv order and initialize fields for provider/MCP, all Ask modes, summaries including an absent prompt, debug-file precedence and config overrides. Full inherited environment/cwd and Node-script launch are subprocess-tested. Registered AskClaude tests cover all thinking parameters. See [SDK launch proof](../bridge-sdk-proof.md); broader MCP control-frame and transcript proof remains open.
- **Lighter-proof limits.** Native partial tool JSON uses the shared incremental parser while source keeps the previous arguments until `JSON.parse` succeeds. Source tool-result extraction normalizes empty/unsupported blocks to a blank text block, while native sends typed content directly. Wrong-type/pathological JSON remains outside the standing parity bar, but ordinary incomplete JSON and empty tool output are realistic and still require regression tests.

## Imported tool-pair repair

Audit found that native imports bypassed source `index.ts:610` / cc-session-io `repairToolPairing`. The authorized fix adds `repair.rs` and calls it before prefix selection and RecordWriter imports. It preserves original non-tool fields and UTF-16 IDs, inserts missing results in Set order, removes orphan results and emits the source first-message placeholder when needed. The public conversion wrapper remains unchanged.

`fixtures/repair-oracle.mjs` ran with installed `tsx` and `cc-session-io@0.4.0` under `/home/osso/Repos/pi-claude-bridge/node_modules`. Byte-identical snapshot `convert.ts` and `skills.ts` copies used `/tmp/bridge-diag-convert-oracle` only to resolve those dependencies; the snapshot stayed read-only. Frozen cases cover orphan uses, orphan results, interleaved pairs, mixed matched/orphan results, duplicate IDs and unpaired UTF-16 IDs. A SessionStore import test first failed on the missing synthetic result and now matches source-written message records; a pure repair test compares exact repaired values. 83 bridge tests passed (`/tmp/bridge-repair-{red,green}.log`).

The approved conversion observer keeps `convert_messages` output unchanged and returns real pre-repair messages plus stats computed at source conversion branches. A RED/GREEN test asserts the five complete ordered debug lines for a dropped thinking block, sanitized ID and inserted synthetic result. `logging_tools.rs` observes extraction and projects source delivery-map transitions without changing actor result processing. 84 bridge tests passed (`/tmp/bridge-conversion-tests.log`).

`reportLeaks` now observes shared per-query activity, pending MCP handler counts and live prompt handles. Counts enter after subprocess construction, change at handler insert/removal/release, and leave at final settlement. The source trigger is **before** session-shutdown cancellation/clear (`index.ts:3006`), not after cleanup. A fake parked tool query first failed on the missing warning, then produced the exact `contexts=1 pendingToolCalls=1 promptStreams=1` line; a second shutdown after settlement produces no further warning (`/tmp/bridge-leak-{red,green}.log`).

Missed-steer handling now matches `index.ts:1988–2029`: absent/rejected prompt pushes mark rebuild, preserve the completion cursor's rebuild flag and still deliver MCP replies. A test-only steer-write seam models the source prompt-stream boundary while leaving the real subprocess control channel alive. Both fake-process cases assert exact UTF-16 results received by Claude, the rebuild state and all four failure-path log lines. The initial test failed because the old path sent the lost steer into the fake MCP response slot. Closed **OS stdin** instead kills both paths; its query error is matching transport failure, not a parity gap (`/tmp/bridge-steer-{red,green}.log`).

## Logging call sites

Audit in progress against extension snapshot `38d84cb549cce66dd76d45a5020d132f74780393`. Paths below are relative to `claude_bridge/`. Debug calls use the existing `Logs::debug` gate (`CLAUDE_BRIDGE_DEBUG=1`) and `CLAUDE_BRIDGE_DEBUG_PATH` or the agent directory's `claude-bridge.log`. Diagnostics remain unconditional at `claude-bridge-diag.log`; auth and shape keep their existing private JSONL sinks. This checkpoint is not exhaustive logging parity.

| TS call site (`src/index.ts`) | Native call site | Fields / condition | Proof |
| --- | --- | --- | --- |
| 2195 provider entry | `provider.rs::BridgeProvider::stream` | activeQuery, lastMsgRole, isReentrant | fake normal query exact line |
| 1971 consume loop exit | `provider.rs::QueryActor::run` | wasAborted, capturedSessionId (8 chars) | fake normal query exact line |
| 2602 final stream completion | `provider.rs::BridgeProvider::stream` completion task | successful run, before finish | fake normal query exact line |
| 2744 AskClaude launch | `ask_claude.rs::AskRun::run` | mode, model, cliModel, effort, isolated, resume, skills, promptLen | fake shared-history query, imported and existing-native |
| 2807 AskClaude native tool | `ask_claude.rs::AskRun::run` | tool_use content_block_start name | insertion; tool-flow proof pending |
| 2832 AskClaude result usage | `ask_claude.rs::AskRun::run` | in, out, cacheRead, cacheWrite, turns; only with usage | insertion; usage-flow proof pending |
| 2849 AskClaude completion | `ask_claude.rs::AskRun::run` | stopReason, resultSubtype, sdkMessages, textDeltas, responseLen, toolCalls | fake shared-history query exact field order |

Second logging group adds `logging_stream.rs` observers for stream/assistant fallback, skipped native tools, unknown SDK/event/block types, discarded blocks and usage. `sessions.rs` logs clean/reuse/rebuild decisions and post-write paths. Provider hooks cover rewrite/orphan/setup, rate-limit/error results, MCP wait/queue, result delivery and cancellation. The source `empty_prompt` diagnostic is unconditional and has an exact ordered JSON assertion. Fake normal, Pi-read round trip, UTF-16 error, cancellation and AskClaude flows now assert log output. Integration fixtures use distinct per-directory Pi session keys: their previous shared key raced on the PID-scoped association temp filename. This is test isolation, not a runtime persistence change. Second-group proof: 72 bridge tests passed; workspace check, strict extensions clippy and fmt check passed (`/tmp/bridge-diag-{group4-tests,check-2,clippy-2}.log`). Full per-site table remains in progress. The next group adds query settlement/error/finalize logging, exact fresh-query fields, all three session rebuild-case lines, carried-attachment diagnostics, session completion and provider registration decisions. The normal-query test first failed on the missing settlement line and now passes. Workspace check, strict extensions clippy and fmt passed (`/tmp/bridge-diag-{check-3,clippy-3}.log`). Audit found that the native importer lacks source `repairToolPairing`; main authorized a separate behavior-fix commit with TS-derived fixtures before adding conversion-report logging.

Checkpoint proof: both focused subprocess tests passed after observed assertion failures; workspace all-target check (`-j3`), strict extensions all-target/all-feature clippy and fmt check passed. Logs: `/tmp/bridge-diag-check-1.log`, `/tmp/bridge-diag-clippy-1.log`. Inventory: 686 rows / 186679 lines, unchanged. These checks cover only this checkpoint; later logging insertions invalidate the affected scope.

## Proof ledger

- `7c4960437`: real-binary package discovery/startup and offline Opus/Haiku listing; workspace all-target check, fmt and strict changed-crate clippy passed.
- `e66b51bd9`: 33 bridge tests, including TS/Rust bidirectional state and cc-session-io JSONL; workspace/fmt/strict extensions clippy passed.
- `0ddafa3cd`: 40 bridge tests; offline HTTP and CLI refresh, capacity and throttled pending requests; workspace/fmt/clippy passed.
- `4b4826a72`: 47 bridge tests; summaries/lifecycle/rebuild/terminating tools; workspace/fmt/clippy passed.
- `c4bc63836`: 53 bridge tests; `/tmp/claude-bridge-item5-gate-{1,2,3,4}.log` records focused tests, workspace all-target check `-j3`, fmt and strict extensions clippy. Code unchanged by this audit documentation commit; proof remains valid. Source inventory is 686 rows / 186679 lines.


## Stream partial JSON and empty-result slice (2026-10-25)

| Source boundary | Native behavior | Proof |
|---|---|---|
| `index.ts::parsePartialJson`, `processStreamEvent` input deltas and block stop | Strict lossless JSON parse replaces arguments only on success. Incomplete/invalid accumulated JSON retains initial or last successful arguments; stop applies tool argument mapping and removes stream metadata. | Native RED/GREEN delta/stop tests; unchanged TS helper executed with tsx, including nested input and unpaired UTF-16 (`fixtures/stream-semantics-oracle.{mjs,json}`). |
| `extract-tool-results.ts::toolResultToMcpContent` | Live MCP results filter empty text, unsupported blocks and incomplete images, retaining valid text/images; an empty result becomes one blank text block. Transcript conversion still uses its distinct empty string representation. | Frozen TS conversion/extraction values compared natively; offline child asserts the complete MCP response for both empty arrays and empty text, with `isError: true`. |
| `processAssistantMessage`, successful `result`, `finalizeCurrentStream` | Empty completed assistant text is skipped, missing assistant content does not record usage, streamed empty text remains observable, result-only empty/missing text emits one empty text block, and contentless turns start before their terminal event. Empty synthetic text is omitted. | Offline child modes assert event order, final content, errors and usage for seven cases. |
| `resultErrorText` success-shaped `is_error` | Empty/missing result text uses `Claude Code reported an error`; nonempty failure text retains UTF-16. Neither creates successful text content. | Offline empty/nonempty failure modes, with exact error text and zero/retained usage assertions. |

This closes the baseline partial-tool-JSON and empty/unsupported live-result normalization findings above, not the other baseline findings. No network or live credentials were used. RED evidence: `/tmp/bridge-stream-red.log`; pre-merge GREEN/gates: `/tmp/bridge-stream-precommit-{tests,check,clippy,fmt}.log` (69 bridge tests; workspace all-target check `-j3`; strict extensions all-target/all-feature clippy; formatting).


Integrated checkpoint: stream implementation `895647444` plus `native-ext/claude-bridge` at `3d6eb518ca15c5b24095957493833eac19dff841`. `/tmp/bridge-stream-merged-{tests,check,clippy,fmt}.log` covers the combined tree: 75 bridge tests pass; `cargo check -j3 --workspace --all-targets`, `cargo clippy -j3 -p pi-coding-agent-extensions --all-targets --all-features -- -D warnings`, and `cargo fmt --check` pass. Manual readability review covered the stream/conversion/provider changes and tests. Subsequent changes to this checkpoint are documentation only.
