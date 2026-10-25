# RPC client traceability

Frozen source: `packages/coding-agent/src/modes/rpc/rpc-client.ts` at [common pin4f9e6f8a4](../source-lock.json). [Contract](../../specs/rpc-client.md); [protocol declarations](rpc-types.md). Revalidation is in progress; this is not a whole-module acceptance claim. Command/revision/invalidation ledger: `/tmp/pi-rust-repin-lossless-interactive/rpc-client-proof-ledger.md`.

## Coverage matrix

| Frozen surface/lines | Native behavior | Proof level |
|---|---|---|
|26–43 options,80–155 start | Every option string stays JsString until Node's OS UTF-8 boundary; file/argv/cwd/env validation and JS own-key/inherited override ordering; executablePath;100ms check; original synchronous errors vs retained asynchronous failed-child identity | Actual-source/private-process RED→GREEN for missing executable, empty/NUL file, raw argv/cwd/env, mixed validation precedence, inherited/numeric env keys; native executable/argument-order proof. Signal-only startup/cache actual-source RED exposed native over-rejection; literal exitCode-only startup and cached send error GREEN atf373abfd (TERM/USR2). Shared signal_name matches all31 Node Linux constant numbers, canonical alias spellings proven by private exits |
|158–181 stop | One owned shared completion, not mutex-serialized retries; detach callbacks while preserving/draining live stdout; reject pending, TERM→KILL→failure; failed stop retains child and next send reports previous failure | Actual-source concurrent-stop source2/native4 RED→GREEN; real writing child's EPIPE RED→GREEN; existing native lifecycle tests retained for final gate |
|119–135 exit/error/stdin | Process identity guards; once-only child error observer; first error wraps message/rejects pending, repeated error uses original native uncaught authority; queued FIFO writes don't block stop and EPIPE rejects every pending request | Real non-reading2MiB stdin and closed-stdin actual-source RED→GREEN. Once EPERM/repeated-uncaught, ESRCH and native wait EIO GREEN at76b2e465; injected EINVAL/ENOSYS native Result GREEN |
|186–208 onEvent/stderr | Live listener-array iteration and synchronous unsubscribe; Result callbacks; raw JSON event values; per-chunk Node Buffer UTF-8 decoding | Source/native null, primitive, throwing listener, live removal and terminal-unsubscribe RED→GREEN. Unhandled stderr read error authority source/native GREEN; actual broken stderr mirror EPIPE RED native status7/source uncaught exit1; error routing implemented. Actual Node default unhandled write on broken fd2 exits1; native canonical reporter previously aborted134 when eprintln failed. Approved shared hook now uses a fallible diagnostic write with the same fatal exit and no alternate sink; GREEN pending |
|523–545 handleLine | Parse raw JsValue; ignore non-JSON/null.type and caught listener throws; strict string correlation of generated ids; return matched envelopes without DTO conversion, otherwise forward original values | Actual-source raw envelopes (unknown fields/order/raw units/non-boolean success) and listener/primitive process cases GREEN |
|586–627 send | req correlation, id appended last,30s response timer; source guard order and cached errors; FIFO byte delivery independent of response waits | Native wire-order/exit/stop tests; source/native write-backpressure and EPIPE cases GREEN. Request timeout/edge lifecycle audit open |
|631–639 getData | Boolean(success), new Error(error) with undefined/default and ToPrimitive TypeError; raw/undefined-aware data before declared decoding; exact nullable field-access errors | Actual-source success truthiness/error construction GREEN; remaining typed return-boundary cases pending |
|469–515 wait/collect/promptAndWait | First agent_end even with willRetry; remove terminal subscription during notification; binary64 ms through shared Node timer normalization; collection begins before prompt | First-end/live-unsubscribe and14 timer cases actual-source GREEN. Private-process canonical-error RED: native returned status7 without reporting source's unobserved timeout; 8ef7256d GREEN: independent worker/observed marker; isolated canonical process exit for both timeout-before-ACK and prompt-rejection-before-timeout. Source recovery probe's PromiseRejectionHandledWarning remains visible |

## Every declared command method

The old-only `get_available_thinking_levels` method/DTO is removed. Each row below sends the literal source body; optional properties are omitted, never null-defaulted.

| Methods | Wire/return contract | Proof |
|---|---|---|
|prompt, steer, followUp, abort, interrupt | Send-only; failed response.success is ignored by these wrappers | Native command/process tests; raw send process differential; whole RPC mode process matrix |
|newSession | parentSession?, `{cancelled}` | Actual frozen-server getter fixture, independent fresh session |
|getState | RpcSessionState with domain Model and all declared state fields | Actual frozen-server exact parsed wire-value comparison |
|setModel, cycleModel, getAvailableModels | Declared ModelReference/ModelInfo, not over-strict full Model; raw strings and ordered unknown model properties | Partial-model actual-source RED→GREEN plus actual frozen-server responses |
|setThinkingLevel, cycleThinkingLevel | Send-only level / nullable `{level}` | Protocol branches and actual frozen-server nullable getter |
|setSteeringMode, setFollowUpMode | Literal queue mode | DTO command matrix, RPC server matrix |
|compact, setAutoCompaction | customInstructions? / enabled; CompactionResult | Native compact payload tests; actual frozen-server compaction in its own persisted scenario |
|setAutoRetry, abortRetry | enabled / send-only | DTO and RPC server matrix |
|bash, abortBash | command / send-only; BashResult | Native payload test and actual frozen-server bash response |
|getSessionStats | SessionStats | Native payload test and actual frozen-server response |
|exportHtml | outputPath?; raw `{path}` | Actual frozen server lone-surrogate outputPath RED→JsString GREEN; full getter matrix |
|switchSession, fork, clone | Literal paths/entry id; cancelled or text/cancelled records | Actual frozen-server success in independent fresh persisted scenarios; native clone wire assertion |
|getForkMessages, getEntries, getTree | `.messages` projection / entries/tree with nullable raw leaf ids | Actual frozen-server getter matrix; raw entry/leaf actual-source RED, final GREEN pending |
|getLastAssistantText, setSessionName | `.text` projection nullable raw text / send-only name | Actual frozen-server getter, DTO/server matrix |
|getMessages, getCommands | `.messages` / `.commands` projection | Actual frozen-server getter values, including nonempty messages/commands |

## Typed return-boundary gaps (main ruling2026-10-24)

Every property emitted by the actual frozen server must decode without error or loss. `rpc_client_frozen_server` compares each typed result to its actual logged source response (the field selected by frozen first, where applicable). The first complete matrix passed atd8a1dc7e; the later raw export falsifier exposed/corrected client-only path String. No missing core response field was found in that matrix. Final transport-level rerun remains required.

Foreign/malformed peer values that cannot inhabit the declared return type reject explicitly with Result. Successful decoding must reserialize to the same parsed value; unknown fields are never silently dropped. This approved(c) boundary is per getter:

| Getter | Explicitly rejected nonrepresentable values |
|---|---|
|newSession, switchSession, clone | Missing/non-boolean cancelled; extras no frozen server emits |
|getState | Missing/mistyped state fields, invalid shared enum/domain Model fields, foreign extras |
|setModel | Missing/non-string provider/id; other properties remain raw and ordered |
|cycleModel | Non-null malformed model reference/thinkingLevel/isScoped; foreign outer extras; model extras remain raw/ordered |
|getAvailableModels | Missing/non-array models or missing/mistyped declared ModelInfo fields; model extras remain raw/ordered |
|cycleThinkingLevel | Non-null malformed level or foreign extras |
|compact | Missing/mistyped CompactionResult fields or foreign extras |
|bash | Missing/mistyped BashResult fields or foreign extras |
|getSessionStats | Missing/mistyped SessionStats/context/token fields or foreign extras |
|exportHtml | Missing/non-string path or foreign extras; all UTF-16 string paths are represented |
|fork | Missing/non-string text or non-boolean cancelled; foreign extras |
|getForkMessages | Missing/non-array messages; malformed entryId/text items or foreign item extras |
|getEntries, getTree | Invalid entry/tree records, non-string/non-null leafId or foreign extras; all UTF-16 leaf ids are represented |
|getLastAssistantText | Missing text is rejected, not normalized to null; non-string/non-null text rejects |
|getMessages | Invalid AgentMessage declared fields or foreign extras, including unrepresentable nested typed image/domain metadata |
|getCommands | Missing/mistyped declared slash-command/sourceInfo fields or foreign extras |

Missing/null data followed by a frozen local property access raises the exact TypeError before this boundary; successful missing direct data instead reaches the explicit typed Result error. Send-only methods perform no getData check. The server's own JsValue ingress and unknown echoes are not narrowed by these DTO rules.

## Discrepancies and approved runtime/language boundaries

| Classification | Exact scope |
|---|---|
|DISCREPANCY, preserved | Frozen start :150 checks exitCode only: a signal-only exit succeeds in start but its cached exit error rejects send. Native preserves this distinction, not a stricter startup check |
|DISCREPANCY, preserved | Frozen handleLine :523–545 catches not just JSON parsing but null.type and every listener throw, silently skipping remaining listeners for that line. Native catches Result rejections here only; it does not route these observed errors uncaught. The shared JSONL data callback's uncaught boundary is different |
|Approved language gap | Public Rust SDK futures start on first poll and dropping a caller-owned future cancels its continuation (main2026-10-24 reconfirms the pre-repin first-poll boundary in this document atb9297a4d, Exclusions and deviations). A queued request is still sent FIFO and its response stays correlated/discarded, not misrouted. Internal collectEvents promise lifetime is not covered by this gap: its independent worker is retained on prompt failure; explicit cancellation still aborts caller-owned collection work |
|Approved runtime gap | Child envp order is std Command's sorted order rather than Node insertion order; names/values and pre-spawn validation order remain exact. See [accepted shared-spawn item](child-process.md#open-shared-spawn-item) and AGENTS user decision2026-10-24. No backend or test exclusion added |
|Approved timer gap | TimeoutOverflowWarning, TimeoutNegativeWarning and TimeoutNaNWarning are not emitted natively; normalization/error values match. Source warnings remain visible in proof logs; OS timer precision/interleavings retain the shared runtime boundary |
|Approved runtime-oddity gap | requestId uses native u64, not JS Number's stagnation after2^53 increments. Main2026-10-24 classified this unreachable private-state behavior as a runtime oddity; no collision-promise machinery is added |
|Approved injected-fault gap | EINVAL/ENOSYS for fixed valid TERM/KILL is unreachable on supported platforms without an injected/backend fault (main2026-10-24 ruling). Native stop returns the original immediate Result error and does not port Node's orphan termination timer after a synchronous throw |
|Native-only proof/gap | A Tokio child wait failure has no frozen child-process callback counterpart. It must reject pending explicitly as a process error rather than disappear. Fault-injected boundary proof, not a claim to have produced a real kernel wait failure |
|Permission proof limitation | Ordinary same-uid private children do not supply a real setuid-free EPERM setup here. Source Node handle-return injection and native KillFn EPERM injection prove once-error/repeated-uncaught behavior; no real-kernel permission-failure claim or ignored test is added |
|Shared approved boundaries | JS stacks omitted, owned return values instead of JS aliasing, opaque scalar domain metadata, and host/runtime-dependent independent producer interleavings follow AGENTS. Built-in collection/wait consumers rely on live unsubscribe and are implemented explicitly |

## Pending finalization

No pair checkpoint is ready yet. Finish internal promise/error/lifecycle audit; publish exact per-command/branch proof and current full runtime/gates; then reconcile the two coverage rows. No push, deployment, live provider/state, new dependency or spawned agent is authorized.
