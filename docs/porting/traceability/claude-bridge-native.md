# Native Claude bridge — work in progress

Source: extension snapshot `38d84cb549cce66dd76d45a5020d132f74780393`, outside the frozen core inventory. Requirements: [native bridge spec](../../specs/claude-bridge-native.md).

| Feature | Current coverage | Proof |
|---|---|---|
| Config merge / startup marker | Nested override; unknown properties and UTF-16 retained; malformed config not overwritten | Native behavioral tests |
| Model catalog | Anthropic projection, source filtering/order, zero cost, context gates | Native behavioral tests with Opus 5.5 and Haiku 5.5 |
| Native identity | Hidden factory, package settings discovery, provider/models registered during activation | Actual settings/packages/resource-loader test |
| Streaming | Text, thinking, tool arguments, partial usage, tool boundary and result errors | Native wire-frame tests |
| CLI / MCP | Local child, initialize, stream-json, tools/list, ID-paired tools/call; Pi read round trip | Offline fake executable through registered provider |
| AskClaude / prompt capture / sessions / accounts / usage / summaries | Not complete | No completion claim |

Mechanical core API additions: readonly `ExtensionApi::cwd()` and discovered `extension_path()` accessors, for project config and package-relative executable lookup. No loader-resolution implementation is duplicated.

Proof ledger (local artifacts):

- Foundation `4266e8c9a`: targeted RED/GREEN in `/tmp/claude-bridge-red-models-2.log`, `/tmp/claude-bridge-red-config.log`, `/tmp/claude-bridge-green-foundation.log`; workspace all-target check and strict extension-crate clippy passed in `/tmp/claude-bridge-foundation-{check,clippy}.log`. Later additions invalidate their integration scope.
- Provider checkpoint: native decoder RED `/tmp/claude-bridge-red-stream.log`; registration RED `/tmp/claude-bridge-red-registration.log`; full fake-process RED `/tmp/claude-bridge-red-process.log`; 10 tests GREEN `/tmp/claude-bridge-process-green-3.log`. Workspace check `/tmp/claude-bridge-provider-check.log` passed; clippy found one redundant `Ok(...?)`, fixed before commit. These proofs cover only the rows above, not full source parity.

Pending source discrepancy decision: extension reads `systemPromptOptions.sections` but not frozen pi's `rulesContent`; literal source projection omits those user rules. Main is obtaining the user's decision. Do not silently reinterpret it.
