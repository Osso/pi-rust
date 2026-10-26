# Native Claude bridge — work in progress

Source: extension snapshot `38d84cb549cce66dd76d45a5020d132f74780393`, outside the frozen core inventory. Requirements: [native bridge spec](../../specs/claude-bridge-native.md).

| Feature | Current coverage | Proof |
|---|---|---|
| Config merge / startup marker | Nested override; unknown properties and UTF-16 retained; malformed config not overwritten | Native behavioral tests |
| Model catalog | Anthropic projection, source filtering/order, zero cost, context gates | Native behavioral tests with Opus 5.5 and Haiku 5.5 |
| Native identity | Hidden factory, package settings discovery, provider/models registered during activation | Actual settings/packages/resource-loader test |
| Streaming | Text, thinking, tool arguments, partial usage, tool boundary and result errors | Native wire-frame tests |
| CLI / MCP | Local child, initialize, stream-json, tools/list, ID-paired tools/call; Pi read round trip | Offline fake executable through registered provider |
| AskClaude | Opt-in registration, schema/defaults, native-mode restrictions, isolated process, progress, action summaries, lossless TUI previews | Fake process through actual registered tool; schema/action/renderer tests |
| Prompt capture | Exact/inherited capture projection; recency/reachable captures; portable context/custom/append/skills; explicit unresolved/unsafe failures; approved user-rules mapping; before_agent_start/agent_start/turn_start hooks | Native inheritance test and exact rules-present/rules-absent test |
| Transcript conversion | Provider/native tool names; tool IDs; text/images/thinking; parallel-result hoisting past steers and empty aborted turns | Native tests and frozen TS provider/AskClaude fixtures |
| Session import | Provider imports prior history into cc-session-io-shaped records and supplies `--resume`; lossless JSONL and recorded Pi associations; prefix/attachment helpers | Fake executable rejects missing resume/history; frozen writer fields and TS reader round trip; parallel-result/prefix/file-attachment tests |
| Session lifecycle / accounts / usage / summaries | Still in progress; lifecycle rewrite/shutdown hooks, AskClaude shared sync, account and summary integration remain | No completion claim |

Prompt checkpoint: RED `/tmp/claude-bridge-red-prompt.log`; 18 targeted tests GREEN `/tmp/claude-bridge-prompt-integrated.log`; the exact rules-present/rules-absent text is asserted by `claude_bridge_prompt_exact_rules_present_and_absent`. The provider uses projected captures, not the raw harness prompt. AskClaude requests only the captured native-read skills block.

Conversion checkpoint: RED `/tmp/claude-bridge-red-conversion.log`; native GREEN `/tmp/claude-bridge-green-conversion.log`; frozen provider/AskClaude fixture proof `/tmp/claude-bridge-conversion-fixtures.log`. `fixtures/session-oracle.mjs` loads copied, unmodified `convert.ts`/`skills.ts` from the locked extension and the installed `cc-session-io` dependency; it uses deterministic clock/UUID/random values only to freeze externally persisted records. Fixture `session-oracle.json` was produced offline. Workspace all-target check, strict changed-crate clippy, and fmt check passed in `/tmp/claude-bridge-conversion-{check,clippy}.log`. Imported-record bidirectional proof now compares native records to this frozen writer fixture and reads native JSONL with the unmodified test-only cc-session-io snapshot (`fixtures/cc-session-io.mjs`, with its MIT license). Generated UUIDs, timestamps and slug are normalized only for comparison; record content, field order and parent links remain checked.

Mechanical core API additions: readonly `ExtensionApi::cwd()` and discovered `extension_path()` accessors, for project config and package-relative executable lookup. No loader-resolution implementation is duplicated.

Proof ledger (local artifacts):

- Foundation `4266e8c9a`: targeted RED/GREEN in `/tmp/claude-bridge-red-models-2.log`, `/tmp/claude-bridge-red-config.log`, `/tmp/claude-bridge-green-foundation.log`; workspace all-target check and strict extension-crate clippy passed in `/tmp/claude-bridge-foundation-{check,clippy}.log`. Later additions invalidate their integration scope.
- Provider checkpoint: native decoder RED `/tmp/claude-bridge-red-stream.log`; registration RED `/tmp/claude-bridge-red-registration.log`; full fake-process RED `/tmp/claude-bridge-red-process.log`; 10 tests GREEN `/tmp/claude-bridge-process-green-3.log`. Workspace check `/tmp/claude-bridge-provider-check.log` passed; clippy found one redundant `Ok(...?)`, fixed before commit. These proofs cover only the rows above, not full source parity.

Approved source discrepancy (user decision 2026-10-25): forward `rulesContent` as `<user_rules>` at the source custom-sections position. See the [spec](../../specs/claude-bridge-native.md).

AskClaude checkpoint: fake-tool RED `/tmp/claude-bridge-red-askclaude.log`; action-summary RED `/tmp/claude-bridge-red-actions.log`; render RED `/tmp/claude-bridge-red-render.log`; 15 targeted tests GREEN `/tmp/claude-bridge-green-ask-render.log`; workspace all-target check `/tmp/claude-bridge-ask-check.log`, strict clippy `/tmp/claude-bridge-ask-clippy.log` and `cargo fmt --check` passed. Later prompt/session/account work invalidates the integration scope, not these bounded behaviors.
