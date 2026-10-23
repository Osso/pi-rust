# Interactive abort consumers

Authority: common-pin interactive-mode, login-dialog, bordered-loader, TUI editor/autocomplete/cancellable-loader modules ([lock](../porting/source-lock.json)). [Shared signal contract](abort-signals.md).

- Every site uses the shared reason-bearing AbortSignal/AbortController. Do not synthesize a different reason, convert arbitrary reason values into default errors, or maintain a second cancellation carrier.
- Install reporting passes an actual shared `AbortSignal.timeout(5000)` to Fetch: branded TimeoutError/code23, not a default controller AbortError. A consumer retaining the signal observes that deadline even after Fetch has completed. Fetch failures remain intentionally swallowed like frozen; no telemetry/statistics policy change or real network test is introduced.
- Escape/login/editor cancellation producers call abort with the reason omitted, preserving the shared default AbortError and first-abort semantics. Callbacks run at the source ordering point, not before signal abort.
- Autocomplete observes cancellation at the frozen async boundaries and returns no suggestions for an aborted request. Do not modify the supplied reason when discarding results.
- A noncancellable bordered loader remains un-aborted; disposal stops animation rather than inventing cancellation. Reason behavior that already matches after the carrier swap needs proof, not a speculative rewrite.

[Current consumer proof matrix](../porting/traceability/interactive-mode.md#abortsignal-consumer-audit-2026-10-22). Whole-mode, whole-editor and whole-autocomplete acceptance remain separate.
