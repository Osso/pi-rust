# Claude bridge SDK launch and effort slice

## Active goal

Match the stream-json launch options used by source `pi-claude-bridge` `38d84cb549cce66dd76d45a5020d132f74780393`: provider, AskClaude and isolated summary argv, environment, cwd, initialize payload and effort selection. Completion requires offline subprocess proof, workspace all-target check (`-j3`), strict changed-crate clippy and formatting, then integration with `native-ext/claude-bridge`. Cancellation, logging call-site coverage and partial JSON algorithms belong to the parallel lanes. No network or live credential use.

## Source authority

SDK: `/home/osso/Repos/pi-claude-bridge/node_modules/@anthropic-ai/claude-agent-sdk/sdk.mjs`, version `0.3.293`, SHA-256 `a980b792353c301eea60d7896704cff21440de7c4aeefc3fe481be079504e203`. The installed file is minified; these are actual physical lines:

- `sdk.mjs:136`, `ProcessTransport.initialize`, `qx`, `g1e`: argv order, summary's `--setting-sources=`, debugFile taking precedence over debug, extraArgs insertion order, dash-prefixed extra values, Node entrypoint for `.js/.mjs/.tsx/.ts/.jsx`, removal of NODE_OPTIONS, DEBUG handling and entrypoint default.
- `sdk.mjs:142`, `fT`: SDK version and case-insensitive session-state environment defaults; preset vs custom/absent system prompt projection; JSON settings serialization. These SDK launch functions do not add `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`; the bridge sets it to `1` on `process.env` at activation (`src/index.ts:2891`), so every provider, AskClaude, summary and usage-refresh child inherits it. Native children set it explicitly.
- `sdk.mjs:142`, `mT`: a string prompt (AskClaude `index.ts:2754`, isolated summary `index.ts:933`) is written as `{type:"user",session_id:"",message:{role:"user",content:[{type:"text",text}]},parent_tool_use_id:null}`; native `process::string_prompt_message`.
- `sdk.mjs:138`, `buildInitializeRequest` and `captureSdkMcpServerManifests`: hook registration, sdkMcpServers and in-process tool manifests. SDK servers do not appear in `--mcp-config`.
- Bridge `src/index.ts:1524–1528,2432–2499,2732–2774` and `src/models.ts:50–56`: generic effort table, model override precedence, null/invalid suppression, AskClaude generic-only thinking mapping and the model projection's retained thinkingLevelMap. No bridge path supplies fallbackModel.
- Bridge `src/index.ts:933–949` and SDK `fT`: summaries use model/max-turns in SDK option order, no resume/permissions, empty skills, and a custom system prompt array. An absent summary prompt becomes `[""]`, unlike a preset which omits systemPrompt.

## Capability/proof matrix

| Boundary | Implemented behavior | Offline proof |
| --- | --- | --- |
| Provider argv/init | Exact flag order; native WebSearch; strict on/off; model suffix and dash escaping; settings; resume; partial events; PostToolBatch even without Pi tools | SDK-generated `sdk-launch-provider/tools/configured/debug.json`, compared against actual fake-process argv and parsed initialize |
| AskClaude argv/init | All three disallowed-tool lists; user/project sources; strict before model; isolated persistence; empty skills; preset append | SDK-generated `sdk-launch-ask/ask-read/ask-none.json` |
| Summary argv/init | max-turns/model order; empty tools/sources; strict; isolated persistence; no permission flag; custom/absent system prompt | SDK-generated `sdk-launch-summary/summary-empty.json` |
| Child environment/cwd | Full inherited environment with SDK defaults, NODE_OPTIONS removal and DEBUG handling; bridge suppression variables; selected config directory; exact cwd | Complete environment equality without secret diffs; default and inherited-override test executions |
| MCP initialize | SDK server names plus initialize/tools-list manifest; exact tool descriptions/schemas; no invented CLI MCP config | Captured installed SDK with the source's MCP server construction; native fixture receives the same manifest |
| Provider effort | Model map wins; missing entry uses generic table; null/invalid entry sends no effort; no generic Max/Ultra mapping; summarized display only with effort | End-to-end provider query through fake process, including native model override and suppression cases |
| AskClaude thinking | Omitted/off use CLI default; minimal/low→low, medium→medium, high→high, xhigh→max, independently of model override | Registered tool execution through fake process for every schema option |
| Configured JS CLI | Config path ending in `.mjs` launches through node, without requiring executable permission | Offline Node wrapper delegates only to the fake Python CLI |

`fixtures/claude.py` adds an opt-in `.fixture-record-launch` mode. It records complete argv/environment/cwd/initialize, responds to initialization and returns a fixed assistant result. Existing response modes remain intact. Existing incorrect launch assertions are corrected rather than weakened: summary sources use the exact equals form; explicit debugFile excludes `--debug`; the fixture does not accept an invented traffic variable.

The SDK captures normalize only temporary paths and the nondeterministic control request ID. The committed JSON deliberately omits environment contents; Rust computes the entire expected inherited environment separately. UTF-16 lone units are retained in append and summary text. These fixtures do not claim complete MCP transport, transcript persistence or extension parity.

## Reproduction and proof ledger

From the slice worktree, use argv-style commands through Pyrun:

1. `node crates/pi-coding-agent-extensions/src/native_user_extensions/claude_bridge/fixtures/sdk-launch-oracle.mjs /home/osso/Repos/pi-claude-bridge /tmp/bridge-sdk-regenerated` — nine installed-SDK captures reproduced byte-for-byte. The generator uses only fake Claude and in-process MCP.
2. `cargo test -j3 -p pi-coding-agent-extensions --lib claude_bridge_sdk_ -- --nocapture` — four tests; initial RED reproduced wrong effort/environment, script execution and missing empty-summary prompt; GREEN passes all nine launch variants and effort cases.
3. `cargo check -j3 --workspace --all-targets`; `cargo clippy -j3 -p pi-coding-agent-extensions --all-targets --all-features -- -D warnings`; `cargo fmt --check` — required before every commit. Logs use `/tmp/bridge-sdk-precommit-*.log` and `/tmp/bridge-sdk-final-*.log`.
4. `python3 scripts/source-inventory.py` — required checkpoint count validation. The slice does not alter locked TS coverage counts.

Proof applies to the associated committed tree. A later Rust/fixture change invalidates the affected test/check scope; documentation-only edits do not invalidate code proof. Merging parallel Rust changes requires fresh integration gates.
