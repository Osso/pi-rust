# agent-core system prompt — common pin

Frozen `packages/agent-core/src/harness/system-prompt.ts:1–34`, 4f9e6f8a4. Rust `harness/system_prompt.rs`.

| Frozen lines | Branches / proof |
|---|---|
| 3–5 | Visible skills filter; empty result; order/duplicates. `skill_formatting` tests + utility differential. |
| 7–24 | Exact preamble, indentation, metadata-only blocks, LF join and no terminal LF. Exact-output tests + differential :13061–13064. |
| 27–34 | Sequential XML escapes, including already-escaped input. Entity-order test + differential. |

Skill metadata and output are native JsString; content is deliberately not included. Eight shared formatting tests GREEN; retained utility differential passes against common pin. Logs `/tmp/pi-rust-repin-delta-agent-core/utility-loader-green.log`, `utils-native-green.log`.
