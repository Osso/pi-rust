# Extension examples — Phase E4 batch B

Pinned source: `532f7d12`, `packages/coding-agent/examples/extensions/`.
Shipped sources compile directly via test `#[path]`. Embedders explicitly register
factories in `ExtensionRegistry`; no default loading or dynamic TS discovery.
No new dependencies. Fixtures use in-memory credentials/session/model state and
network-disabled runtimes. TS oracle loads the unchanged example with real
`loadExtensionFromFactory` and `ExtensionRunner`; no handler/API mocks.
UI contexts capture observable effects, supplying only explicit selection replies.

| Source / lines | Rust example / factory | Test target / proof |
| --- | --- | --- |
| widget-placement.ts:3–9 | `examples/extensions/widget_placement.rs::widget_placement_extension` | `example_widget_placement`: no-UI branch, ordered above/default and below/explicit widgets, repeat start |
| working-message-test.ts:17–25 | `examples/extensions/working_message_test.rs::working_message_test_extension` | `example_working_message_test`: exact ANSI message, one brown-dot frame, no interval; repeated starts, no-UI runtime no-ops |
| protected-paths.ts:10–30 | `examples/extensions/protected_paths.rs::protected_paths_extension` | `example_protected_paths`: write/edit block, all three literal substrings, warning only with UI, case-sensitive/bare-directory/other-tool bypass |
| permission-gate.ts:10–34 | `examples/extensions/permission_gate.rs::permission_gate_extension` | `example_permission_gate`: safe/other-tool bypass, three dangerous patterns, non-UI denial, Yes/No/cancel/case-sensitive choice, ASCII boundaries, BOM whitespace |

Source behavior retained, not hardened: protected paths are substrings, not resolved
paths (`.env.backup` blocks, bare `node_modules` does not). Permission patterns do
not match `rm -fr`; choice must equal `Yes`. Invalid JS argument types are outside
the native string contract; invalid fixture fields produce explicit native errors.

Proof limits: working-message test proves source setter order/values, not interactive
loader-recreation persistence (owned by the host UI). No dangerous commands run.
Every example above has one observed missing-module RED and one passing runtime
comparison; registration alone is inert. Development ledger and full outputs:
`/tmp/pi-rust-ext-examples-b/ledger.md`, `NN-*.log`.

Remaining examples: provider-payload, input-transform, input-transform-streaming,
bash-spawn-hook. Input date locale decision approved 2026-10-20: pin en-US date,
retain local time zone; shared helper and differential fixtures follow.
