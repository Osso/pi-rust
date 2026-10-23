# Harness types — common-pin traceability

Authority: frozen `packages/agent-core/src/harness/types.ts`, `4f9e6f8a4` (838 lines). Rust represents interfaces in `harness/{types,agent_harness_types,session/types}.rs` and compaction records, with root wildcard forwarding. Native support records name TS inline structural types; no old-only APIs are implied.

| Frozen exports/lines | Native representation and behavioral proof |
|---|---|
| Result/ok/err/getOrThrow/getOrUndefined,6–28 | Standard `Result<T,E>`, `Ok`, `Err`, `?`, `.ok()`; no wrapper helpers. Returned-vs-rejected promise distinction is separately `OperationFailure`/`SessionFailure`, proven by deleted-cwd real subprocess and caller matrix. |
| Skill/PromptTemplate/resources/stream options and patches,46–106 | Native JsString human fields, optional resource vectors, explicit unchanged/clear/set patch; loader/raw-text/frozen harness option-patch tests. |
| FileKind/error codes/error classes/FileInfo,108–250 | Typed enums and native records with exact names/messages/causes; `harness_errors`, `compaction_errors`, node-env filesystem/spawn/cause differentials and session invalid-file matrices. JS stack omission follows approval. |
| FileSystem/Shell/options/ExecutionEnv,252–332 | Async traits and named inline-option records, native error carriers, AbortSignal; POSIX node-env/readline/FIFO/shell capture differentials. Native binary/file strings encode only at Node OS boundaries. |
| Session entry variants/context/metadata/storage/repo interfaces,334–491 | Native tagged SessionTreeEntry, SessionStorage/Repo and generic metadata; tree/repository/JSONL differentials, native raw close/reopen and duplicate-ID bug proof. |
| Phases, pending writes and own/combined events,493–662 | HarnessPhase, pending native entries, HarnessEvent variants plus AgentEvent wrapper; frozen working-harness turn/queue/write/save-point/abort/settled matrices. |
| Hook result map and named result interfaces,663–704 | HarnessHookResult tagged result with concrete ToolResultPatch/SessionBeforeCompactResult/SessionBeforeTreeResult; frozen tool/native patch/payload undefined/navigation matrices. |
| Prompt/abort/compact/navigation/settings/preparation/file/branch result records,727–798 | Named harness/compaction records; working-harness busy/abort/idle/navigation/compaction differentials and focused native failure/concurrency tests. |
| AgentHarnessOptions and forwarded AgentHarness,800–838 | Generic options, callbacks and native runtime; constructor/default/auth/tools/resources/system-prompt/request/persistence matrices. |

## toError conversion reachability (30–38)

Frozen behavior is Error `instanceof` passthrough, string -> Error(string), otherwise Error(JSON.stringify(value)), falling back to String(value) only if stringify throws. Plain error-lookalike objects do not pass through. Current Rust callers expose only typed errors or explicitly string-valued callback rejection; there is no arbitrary JsValue thrown-error boundary. No unused converter or claimed nonstring JSON conversion proof is added (parent acceptance2026-10-22).

| Frozen callers | Native boundary/evidence |
|---|---|
| nodejs.ts83/326/356/366 | Native IO/spawn `io::Error` -> NativeNodeError; stdout/stderr callback returns JsString and becomes NativeNodeError(Error,message), exactly the source string branch. Node error metadata/raw callback cause differential. |
| skills.ts315/prompt-templates.ts212 | FileSystem `OperationFailure<FileError>` and NodeRejection; typed native cause is retained. Loader/cwd rejection differential. |
| jsonl-repo.ts117 | FileError/NodeRejection from FileSystem; original typed rejection or returned error retained by SessionFailure. Repository caller matrix. |
| jsonl-storage.ts64/92 | Lossless JSON parser returns typed ParseError; header/entry invalid-JSON fixtures assert errors. |
| shell-output.ts18 | ExecutionError or NodeRejection; original typed native cause survives catch, with no serialized Error-object coercion. Shell/cwd rejection differential. |
| agent-harness.ts130/583/981/986/991 | System-prompt/hooks/listeners/turn/reporting reject with AgentHarnessError/AgentError, session/environment errors convert into those typed carriers; aggregate cause vector stores typed Error members. Working-harness failure/abort/settled persistence/concurrency proof. |

General owned-value, immutable-event snapshot and typed-input exclusions are recorded in AGENTS.md. This type module's mapping is complete; separate runtime coverage rows still track unproven session/compaction/harness branches rather than inheriting a full parity claim from these records.
