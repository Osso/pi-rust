# Native Claude bridge

## Source and scope

Port `pi-claude-bridge` commit `38d84cb549cce66dd76d45a5020d132f74780393` from the read-only extension snapshot. Implement only the Claude Agent SDK, MCP and cc-session-io behavior used by that extension. Do not execute TypeScript or the SDK at runtime.

## Requirements

- Register a hidden `<native:pi-claude-bridge>` factory under `native_user_extensions/claude_bridge`. Package-name discovery is integrated separately.
- Register the `claude-bridge` provider at extension activation, with the Anthropic catalog projection, model ordering, zero costs, measured context limits and thinking-level mapping of the source. Include `claude-opus-5-5` and `claude-haiku-5-5` when present in the catalog.
- Load global and project `claude-bridge.json`, merge their nested provider and AskClaude settings, and preserve source defaults, notices and persistence.
- Spawn the configured Claude executable using the SDK's stream-json arguments and control protocol. Retain exact UTF-16 in content, errors, JSON arguments, wire messages and persistence.
- Expose Pi tool schemas through in-process MCP, pair results by `claudecode/toolUseId`, keep native WebSearch, suppress unrelated native tools, and deliver steering before tool results. Support concurrent sessions, cancellation and shutdown without stranded tool handlers.
- Preserve source transcript conversion, tool-name and argument mapping, usage accounting, native session import/resume/rebuild, prefix preservation, carried attachments and recorded session associations.
- Project Pi context files, custom prompt sections and skills onto Claude Code's preset without forwarding Pi's harness prompt. Preserve inherited prompt captures and explicit unresolved-prompt failures.
- Implement source lifecycle hooks, isolated summaries, compaction and branch summaries. The snapshot registers no slash commands; usage is handled through `claude-bridge:usage-request` and `claude-bridge:usage-all-request`.
- Register opt-in AskClaude with configurable name/label/description/defaults, source parameter schema, mode restrictions, isolated/shared history, native tool action summaries, progress and result rendering, and circular-delegation protection.
- Preserve account-profile validation, credential freshness and writeback, shared selection/quota state, bounded usage-request retries and error reporting. Never use live credentials or the network in tests.

## Verification

Use failing behavioral tests before implementation. Test direct factory registration and an offline fake `claude` executable selected by `provider.pathToClaudeCodeExecutable`. Exercise provider streaming, Pi tool round trips and AskClaude through the fake process, including errors, cancellation and exact UTF-16. Compare wire and persisted records with source fixtures. Before each commit run workspace all-target check (`-j3`), formatting check and strict clippy on changed crates.

## Status and gaps

Implementation is in progress. Unimplemented requirements must not be presented as completed parity.

Named profiles now validate before launch, migrate the freshest legacy credentials once into the canonical private profile directory, share native config entries, and retain native refresh writeback there. Selection and dated quota evidence use the source's per-process account-state files, recovery epochs and process-local undated blocks. Offline tests cover profile queries and quota rotation; the account-state wire format is read and written bidirectionally with the frozen TS module. Both usage request handlers claim synchronously without replacing earlier claims and unsubscribe on shutdown. Usage HTTP requests have three-attempt and five-second wait bounds, jitter, Retry-After handling and sanitized errors. Idle expired grants refresh through the configured executable, including renewal followed by failed inference. Exhausted-account capacity checks and one pending inference attempt per HTTP-throttled profile are integrated; HTTP 429 alone is not recorded as account recovery. Offline fake-process and HTTP fixtures cover these paths. Isolated one-off summaries, branch-summary takeover, new/resume/fork mirror reset, tree/compact rebuild marks, terminating-tool delivery, trailing-assistant REUSE cursor advancement, unowned short-caller preservation and persistence-before-stream-completion are implemented with offline behavioral tests. Compaction takeover: **pending user decision** on adapting the source's old `session_before_compact` return contract to the frozen API's separate `compaction` event. Offline tests now cover cancellation during SDK initialization with child reaping, shutdown with awaited cleanup, orphaned results without a new process, identical tool IDs in independent sessions, parked-query history rewrite/rotation, and all trailing steering messages preceding MCP results. The source-module audit is recorded in [native bridge traceability](../porting/traceability/native-claude-bridge.md), against `c4bc63836`. It does not close parity. Remaining unimplemented requirements, in implementation order:

1. AskClaude cancellation and error-progress cleanup. Shared history/native resume is implemented: non-isolated calls reuse the calling Pi session's mirror or import its resolved history once; isolated calls do not import history. Offline subprocess tests cover both initial import and existing-native resume.
2. Provider ownership is implemented: the first live instance registers at activation; later instances defer until session start and do not replace a provider already present in that session's registry. Independent registries receive their own provider. Rewrite marks reach every live serving instance; clear/reload releases the owner. Offline tests cover deferred registration, independent registries and cross-instance parked-query rebuild.
3. Rate-limit warnings and failure labels are implemented with source wording, per-query 5%/threshold deduplication, allowed/rejected rearming, local reset times and exact UTF-16 errors. Fake-process tests observe the notifications and terminal failure. Cancellation-safe account settlement remains in the cancellation-boundary work.
4. First-TUI-query startup notice and post-write session integrity verification.
5. Source-format auth, shape, debug/CLI-stderr, diagnostic and raw-stream logging.

Further audit findings remain open: selected-profile transcript location; cancellation during tool/control writes, rechecks and summaries; final MCP handler replies on teardown; missed-steer delivery/rebuild; model-specific thinking effort; source-exact SDK argv/init; partial tool JSON semantics; empty tool-result normalization; schema null/whitespace cases; supplied-model cost accounting; and transcript-ingress/bidirectional wire proof depth. The traceability document gives source/native references, exact boundaries and proof status. These are not approved gaps.

Real startup registers the hidden bridge factory through the common native-user registry. The real-binary package-discovery test lists `claude-opus-5-5` and `claude-haiku-5-5` offline. Registered models use the bridge API rather than the catalog's Anthropic API. Startup tests select a missing provider to stop before inference; `PI_OFFLINE` alone does not disable inference.

Approved deviation (user decision 2026-10-25): forward frozen pi's `systemPromptOptions.rulesContent` as `<user_rules>\n{rulesContent}\n</user_rules>` at the position where the bridge appends custom sections. The source bridge reads `sections` (absent in the frozen API) and omits `rulesContent`; preserving that omission would lose the user's global instructions. Test exact projected text with rules present and absent.
