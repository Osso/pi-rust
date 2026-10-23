# Shared reason-bearing abort carrier

Authority: Node builtins and frozen `packages/ai/src/utils/abort-signals.ts` at `4f9e6f8a4`. [Contract](../../specs/abort-signals.md).

| Surface | Observable behavior | Proof in `pi-js-compat/tests/abort_signal.rs` |
|---|---|---|
| `AbortController::new/signal/abort`, signal `default/aborted/reason/abort` | Undefined before abort; omitted reason defaults to DOMException AbortError/code20; supplied null/false/raw UTF-16/object/nonfinite number preserved; first abort wins | `reasons_first_abort_any_and_listener_order_match_node_and_frozen_combine`, direct Node differential |
| `AbortReason::{Value,Error,DomException}`, `value/as_error` | Error branding independent from arbitrary object fields; native metadata and UTF-16 human fields preserved | Same differential, including error-lookalike objects, overridden fields and stack omission |
| `AbortSignal::subscribe`, `AbortSubscription::drop` | Synchronous ordered one-shot delivery; already-aborted subscription returns reason without replay; removal during dispatch prevents invocation | Same differential against Node listeners |
| `AbortSignal::any` | First already-aborted source in argument order wins; otherwise synchronous reason forwarding; empty input never aborts | Same differential against Node any and direct frozen combine import for nonempty multi-source cases |
| `AbortSignal::wait` | Returns the reason; no lost wakeup across registration/abort race | `awaiting_abort_cannot_lose_a_notification_and_composites_keep_sources_alive`, 100 threaded rounds |
| `AbortSignal::timeout` | Exact uint32/integer validation diagnostics; normalized timers; TimeoutError/code23 | `timeout_validation_default_reason_and_integer_domain_match_node`, direct Node differential |
| Composite/timer ownership | Composite retains sources; last owner releases timeout task | Threaded lifetime test plus `dropping_the_last_timeout_owner_releases_its_runtime_task` |
| `NativeNodeError` | Shared IO/error-record authority, explicit name/message, metadata; stack omitted | Existing node-env differential error/cause vectors plus carrier error vectors |

Implementation: `pi-js-compat/src/{abort,node_error}.rs`. `wait` uses Notify with pre-enabled registration; forwarding uses synchronous native subscriptions, not polling tasks. `timeout` requires an active Tokio runtime. Subscription callbacks are native Rust callbacks; panic semantics are not a JavaScript event-listener exception API.

## Boundary migration checkpoint

`pi-ai` public request/Fetch/auth/models/provider signal parameters now use `AbortSignal`; internal cancellers use `AbortController`. Provider wait/aborted consumption is mechanically migrated; source-owned child cancellation combines parent and controller signals. Existing timeout/default-error semantics remain for lane219's RED-first reason audit. The async combine wrapper temporarily retains its prior task-based propagation until that audit; this is not parity evidence or an approved deviation.

`AbortSignal` native equality compares shared signal identity, allowing existing typed auth prompt/options equality without comparing reasons. The focused `signal_equality_compares_shared_signal_not_current_reason` test covers clone versus independent signals.

Core Agent/AgentTool/StreamFn/harness/env/compaction signal fields and all core fixtures now use the shared carrier; Agent and harness run owners use controllers. The in-flight FIFO file test `abort_reasons::in_flight_file_abort_preserves_supplied_reason_in_nested_node_cause` compares null/false/raw UTF-16/native Error reasons to frozen Node env, including ABORT_ERR nested cause. Pre-aborted env operations deliberately retain the frozen generic returned error without a native cause. Core no longer depends on tokio-util.

Coding production APIs, controller owners, tools/session/auth/management/codex extension callers and coding fixtures are mechanically migrated. Old timeout/error synthesis and asynchronous combine semantics remain explicitly unproven; lanes219/221 own their reason audits. Source contains no production CancellationToken, and the direct tokio-util dependencies are removed.

Mechanical cross-lane sites: TUI `autocomplete.rs`, `components/{editor,cancellable_loader}.rs`, `tests/editor_autocomplete.rs`; coding interactive `interactive_mode.rs`, `components/{bordered_loader,login_dialog,model_selector}.rs`; examples `extensions/{custom_provider_anthropic,custom_provider_gitlab_duo,handoff,qna,ssh,timed_confirm}.rs`, `extensions/custom_provider_anthropic/stream.rs`, `extensions/subagent/index.rs`; orchestrator Fetch fixtures `tests/{radius,radius_body}.rs`. These changes update only carrier types, ownership and wait/aborted calls, not their frozen semantic classifications.

At this ordered checkpoint, pi-ai integration fixtures still require lane219's migration. `cargo test -p pi-ai --lib --no-run` proves the production boundary compiles; it does not prove old callers compile or reason semantics.

## Consumer audit and remaining work

- Frozen `abort-signals.ts` forwards `signal.reason` synchronously and exposes cleanup; its production wrapper migration remains with the pi-ai lane. This slice does not claim wrapper cleanup/zero/one-input proof.
- Frozen codex-web-search and codex-image-generation use custom Error timeout reasons and `instanceof Error`; claude-memory-enrich preserves Error reasons but synthesizes its fallback for non-Errors. Branded reasons support those branches without guessing from JSON fields. Consumer migration/tests remain open.
- Node IO nested custom abort causes and core/coding consumers remain on their previous carrier until the following slice. No compatibility aliases are added.
- JS returned-object identity is excluded by the approved owned-value rule; stack is omitted under explicit approval. Raw UTF-16 and arbitrary JSON reason values are not excluded.
