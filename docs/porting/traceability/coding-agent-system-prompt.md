# core/system-prompt.ts

Frozen `4f9e6f8a4`, 236 lines. Native `core/system_prompt.rs`. [Contract](../../specs/coding-agent-system-prompt.md).

| Export/helper/branch | Native mapping | Proof |
|---|---|---|
| BuildSystemPromptOptions / context files | owned typed options, ContextFile with native JsString | complete frozen matrix input/return comparison |
| buildSystemPrompt default/custom | build_system_prompt; one production wrapper over injected-clock implementation | full prompt, no output/path/date normalization |
| local year/month/day | shared Node-compatible date fields; live clock in production | UTC March1, Los Angeles leap-day rollover, year1 fixed clocks on both sides |
| appendUserRulesSection / context / skills | ordered existing helpers, source read gating | omitted/empty/custom/default, newline separator, visible/hidden skills, native UTF-16 fixture |
| buildPyrunAgentGuidelines / guideline assembly | source spawn_agent+pyrun_eval line followed by supplied guidelines; ordered trim/dedupe | missing/one/both tools, background/wait guidance and duplicate/blank/raw bullets |
| selected/default tools, snippet/append truthiness | empty strings omitted; whitespace stays truthy | source matrix includes empty read snippet, whitespace bash snippet and empty append in both branches |
| documentation and final suffix | exact common-pin text; native Node path decode; date then cwd without trailing newline | fixed same package path and exact full native-string comparison |

RED: q15-system-prompt-fixed-clock-red.log compares the actual frozen module under fixed Date/TZ against native missing-date/post-pin-doc behavior. Production fix and final GREEN are pending acceptance. The native probe uses lossless input/output and the approved injected clock, not output normalization.

RPC usage root: q15-faux-context-pair.json captured exact provider-ingress data. Source serializes2310 UTF-16 units→578 tokens; native2388→597. Prompt+30 and tools+48 explain all78 units; no estimator adjustment is made. The old TS fixture set required label to undefined through unknown/as-never. Parent2026-10-23 classified that invalid input under the August12 exclusion: oracle now supplies actual frozen builtin label=end_turn/executionMode=sequential, matching the existing native fixture, without widening AgentTool or stripping provider input. Separate native custom-prompt trailing newline drift was also corrected; default prompts had no trailing newline on either side.
