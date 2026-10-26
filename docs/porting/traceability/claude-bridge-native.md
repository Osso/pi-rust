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
| Prompt capture / sessions / accounts / usage / summaries | Not complete; AskClaude shared history and skills depend on these rows | No completion claim |

Mechanical core API additions: readonly `ExtensionApi::cwd()` and discovered `extension_path()` accessors, for project config and package-relative executable lookup. No loader-resolution implementation is duplicated.

Proof ledger (local artifacts):

- Foundation `4266e8c9a`: targeted RED/GREEN in `/tmp/claude-bridge-red-models-2.log`, `/tmp/claude-bridge-red-config.log`, `/tmp/claude-bridge-green-foundation.log`; workspace all-target check and strict extension-crate clippy passed in `/tmp/claude-bridge-foundation-{check,clippy}.log`. Later additions invalidate their integration scope.
- Provider checkpoint: native decoder RED `/tmp/claude-bridge-red-stream.log`; registration RED `/tmp/claude-bridge-red-registration.log`; full fake-process RED `/tmp/claude-bridge-red-process.log`; 10 tests GREEN `/tmp/claude-bridge-process-green-3.log`. Workspace check `/tmp/claude-bridge-provider-check.log` passed; clippy found one redundant `Ok(...?)`, fixed before commit. These proofs cover only the rows above, not full source parity.

Approved source discrepancy (user decision 2026-10-25): forward `rulesContent` as `<user_rules>` at the source custom-sections position. See the [spec](../../specs/claude-bridge-native.md).

AskClaude checkpoint: fake-tool RED `/tmp/claude-bridge-red-askclaude.log`; action-summary RED `/tmp/claude-bridge-red-actions.log`; render RED `/tmp/claude-bridge-red-render.log`; 15 targeted tests GREEN `/tmp/claude-bridge-green-ask-render.log`; workspace all-target check `/tmp/claude-bridge-ask-check.log`, strict clippy `/tmp/claude-bridge-ask-clippy.log` and `cargo fmt --check` passed. Later prompt/session/account work invalidates the integration scope, not these bounded behaviors.
