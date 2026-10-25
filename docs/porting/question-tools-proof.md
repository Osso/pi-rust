# Whole ask-questions / panel checkpoint

Reference: unmodified `4f9e6f8a4`. Date: 2026-10-24. Production candidate: `a3647524`; final fixture correction: `4973bb49`; round-2 parent `4d633aa1` merged as `9a9c3777` before final gates. This closes these two module slices, not whole-project acceptance.

| Capability | Current proof |
|---|---|
| Schema/descriptor, explicit blocking and guard order, validation, results/metadata and answer enumeration | Full factory descriptor, 12 source/native validation/result cases, missing-context/empty DB guards. Actual guard ordering RED precedes fixes; final exported missing-context TypeError branding has its own RED→GREEN. |
| Panel focus/navigation/Other/toggle/disabled submit/custom bindings/finished-once | 10 complete exact ANSI/UTF16 histories, 99 frames; 8 retained plain-layout histories. Private raw aggregation unit passes. No panel production edits were needed. |
| Stateful call/result rendering | 16 exact color/raw/state frames; actual retained-result RED→GREEN. Existing result component is retained while the call row receives answers. |
| FIFO/abort lifetime and durable queued delivery | Agent abort retains the pending panel through settlement; 3 private DB histories retain captured session/agent IDs across a real manager switch. Actual enqueue failure and notification-throw child histories preserve results, logging, absent failure record and subsequent delivery. |
| Rejection identity and runtime text | 7 actual frozen direct-factory/native ContextTool/real native+frozen agent-loop histories retain raw string, TypeError/code, null, bool, number, lookalike object and missing-context TypeError. The approved coercion-fatal gap is explicit below. |

## Proof ledger

Artifacts and exact command/revision history: `/tmp/pi-rust-repin-core-tools/ledger.md`.

- `question-round2-runtime.log`: current post-split question/panel/guard/queue/rejection and full summarize/handoff histories pass: 17 tests, plus the ignored child helper explicitly exercised by the outer test. This retains all seven direct-factory/runtime histories and the coercion shape probe.
- `question-round2-raw-units.log`: both formatter/panel aggregation units pass in their new `pi-coding-agent-tools` package. `custom-wrapper-retained.log`: 14 retained wrapper tests; existing Error text assertions are unchanged.
- `custom-interactive-host-regressions.log`: 17 real interactive UI host tests pass. DOOM 5, overlay QA 10 and overlay caller 1 pass. Final summarize/handoff histories both pass in `custom-session-api-boundary-green.log`.
- `question-round2-workspace-summary.json`: workspace/all-target/keep-going check passes, 0 errors/warnings, at `9a9c3777`. Post-split tools/interactive/facade all-target/all-feature warning Clippy has 0 changed-primary-span findings (`question-round2-clippy-summary.json`, 1357 total diagnostics in the selected packages). Existing project-wide lint debt is not waived or claimed clean.
- Protected common-reference strict TypeScript and Biome pass for the new rejection oracle (`custom-final-{types,biome}.log`). Earlier seven-fixture protected checks and six specialized fixture style checks remain valid in the [UI checkpoint](ui-notify-proof.md); the unchanged general session-tools oracle's existing 23 warnings/formatter error are explicit, not suppressed. Manual changed-line Rust/TS readability audit is clean; metrics binary is unavailable. Format passes before each commit.

The custom rejection/fixture work adds no dependency, carrier, fallback or panic-based propagation, and does not change provider production behavior, shared ToolError representation or production registry/DB behavior. The shared SQLite open-error prerequisite and compact callback prerequisite have their own earlier proofs.

Frozen inventory passes (`question-final-inventory.log`): 686 rows / 186679 lines; 454 done / 19 gap / 46 partial / 167 todo. Only these two statuses changed at closure. The initial inventory run correctly rejected the stale displayed summary; the summary now matches the unchanged locked source counts.

## Approved gap

A rejection value whose own `toString` throws is caught by frozen `Agent.prompt`, producing a loop-fatal assistant error and no tool result. Native message-only ToolError instead reports the exact conversion TypeError as a tool error. The actual-source process probe exits 0; no process-uncaught authority is used. [2026-10-24 ingress (c) decision and both asserted shapes](../specs/custom-ui-rejections.md#approved-gap-rejection-value-whose-own-tostring-throws).

## Pre-existing consumer fixture failure

The summarize fixture fails identically on exact parent `4c28cdf4` before the custom migration (`summarize-parent-4c28cdf4-baseline.log`). It used coding-agent provider/model registration instead of the oracle's API-stream override. Setting its API exposed hardcoded test-helper auth; removing auth from ProviderConfig was correctly rejected by production validation. The final fixture uses existing `compat::register_faux_provider`, preserves the static model and AuthStorage, and unregisters after each fixture. Obsolete auth/provider wrappers and the invalid registration path are removed; shared helpers and production validation are unchanged.

[Question trace](traceability/ask-questions.md), [panel trace](traceability/ask-questions-panel.md), [custom rejection contract](../specs/custom-ui-rejections.md).
