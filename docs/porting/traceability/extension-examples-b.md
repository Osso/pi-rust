# Extension examples — Phase E4 batch B

Pinned source: `532f7d12`, `packages/coding-agent/examples/extensions/`.
Shipped sources compile directly via test `#[path]`. Embedders explicitly register
factories in `ExtensionRegistry`; no default loading or dynamic TS discovery.
No new dependency/version; existing Jiff enables production `tzdb-zoneinfo` for
named local zones (previously dev-only). Fixtures use in-memory credentials/session/model state and
network-disabled runtimes. TS oracle loads the unchanged example with real
`loadExtensionFromFactory` and `ExtensionRunner`; no handler/API mocks.
UI contexts capture observable effects, supplying only explicit selection replies.

| Source / lines | Rust example / factory | Test target / proof |
| --- | --- | --- |
| widget-placement.ts:3–9 | `examples/extensions/widget_placement.rs::widget_placement_extension` | `example_widget_placement`: no-UI branch, ordered above/default and below/explicit widgets, repeat start |
| working-message-test.ts:17–25 | `examples/extensions/working_message_test.rs::working_message_test_extension` | `example_working_message_test`: exact ANSI message, one brown-dot frame, no interval; repeated starts, no-UI runtime no-ops |
| protected-paths.ts:10–30 | `examples/extensions/protected_paths.rs::protected_paths_extension` | `example_protected_paths`: write/edit block, all three literal substrings, warning only with UI, case-sensitive/bare-directory/other-tool bypass |
| permission-gate.ts:10–34 | `examples/extensions/permission_gate.rs::permission_gate_extension` | `example_permission_gate` (2 tests): safe/other-tool bypass, three dangerous patterns, non-UI denial, Yes/No/cancel/case-sensitive choice, ASCII boundaries/casefold, exact JS whitespace and dot line terminators |

Source behavior retained, not hardened: protected paths are substrings, not resolved
paths (`.env.backup` blocks, bare `node_modules` does not). Permission patterns do
not match `rm -fr`; choice must equal `Yes`. Invalid JS argument types are outside
the native string contract; invalid fixture fields produce explicit native errors.

Proof limits: working-message test proves source setter order/values, not interactive
loader-recreation persistence (owned by the host UI). No dangerous commands run.
Every example above has one observed missing-module RED and passing runtime
comparison(s); registration alone is inert. Expanded permission fixtures separately
reproduce the Rust-regex mismatch for U+0085, long s, and CR/LS/PS dot boundaries;
explicit JS-compatible classes preserve the pinned patterns without hardening. Development ledger and full outputs:
`/tmp/pi-rust-ext-examples-b/ledger.md`, `NN-*.log`.

Baseline re-pin repair (2026-10-25): the frozen bash-tool object has no promptSnippet or promptGuidelines. The native example now registers only the tool fields that frozen spreads; the real-process metadata/execution differential remains exact.

## Remaining native ports and shared date helper

| Source / lines | Rust example / factory | Test target / proof |
| --- | --- | --- |
| input-transform-streaming.ts:16–39 | `examples/extensions/input_transform_streaming.rs::input_transform_streaming_extension` | `example_input_transform_streaming` (2 tests): real private staged/modified git fixture; steer/no-match/ASCII-boundary bypass, trigger transform, image retention, followUp, failed git and empty stat |
| bash-spawn-hook.ts:13–30 | `examples/extensions/bash_spawn_hook.rs::bash_spawn_hook_extension` | `example_bash_spawn_hook` (1 test): real runtime registration and execution, complete tool metadata, private HOME/profile, injected env, original cwd, stdout and nonzero-exit error differential |
| input-transform.ts:14–43 | `examples/extensions/input_transform.rs::{input_transform_extension,register_with_clock}` | `example_input_transform` (3 tests): interactive/rpc/extension sources, JS trim including BOM/NBSP/U+0085, usage, transformed text/images, ping, time, ordinary/no-match input; shared date differential |
| input-transform.ts:36–38 `Date.toLocaleString()` | `pi-js-compat/src/date.rs::to_locale_string` | `example_input_transform::en_us_date_local_fields_match_pinned_node`: 11 epoch values × UTC/New York, fresh native child processes and same-TZ pinned Node |

Each port has one observed missing-module RED; date helper separately has a
missing-function RED. Source's current clock is a local injectable seam, not a
new Extension API. Default entrypoint uses the real system clock. Oracle freezes
only Date construction for the time fixture; actual source event dispatch remains
unchanged. Node v26.11.1 emits U+0020, not U+202F, before AM/PM.
Locale decision approved 2026-10-20: pin en-US dates, retain local time zone.
Calendar formatting is bounded to Jiff's representable range and available local
zone; unresolved zones/range fail explicitly rather than falling back to UTC.
No broad, lint, type or final acceptance gates run.

## Blocked: provider-payload

Unported; its observed missing-module RED is retained as
`15-provider_payload-RED.log` in the ledger, but the failing test was removed per
main-thread instruction. No production stub remains. TS `provider-payload.ts:16`
serializes the insertion-ordered `event.headers` object. Native
`core/extensions/types.rs:1703–1706` defines `AfterProviderResponseEvent.headers`
as `BTreeMap<String, String>`, irreversibly sorting header keys. For an input record
`{"z-last":"λ","a-first":"value"}`, native API loses the source's order before
this example can observe it. Main confirmed this defect and owns a separate
header-order/API slice and this example; this batch does not change that API.
Before-request payload logging was not shipped in isolation, preserving whole-module scope.

## Re-pin baseline regex repair (2026-10-25)

`permission_gate` and `input_transform_streaming` had retained Rust regex
inline modifiers after moving to JsRegex. Both targets reproduced Invalid group
modifier before handler execution. The factories now use the literal frozen
patterns with `with_flags(..., "i")`; no translation layer or alternate engine.
Both targets pass their 2 existing TS-runtime differential tests, including
casefold, word-boundary, whitespace, dot, and actual Git behavior. No assertions
changed. Readability audit: literal-pattern simplification, no new branches.
