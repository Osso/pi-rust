# Device code polling — common-pin closure

Frozen `packages/ai/src/utils/oauth/device-code.ts`, `4f9e6f8a4`; native `auth/oauth/device_code.rs`.

| Source exports / branches | Native proof |
|---|---|
| OAuthDeviceCodePollResult and Options | Native tagged result/options; callback rejection is Result, not a failed-status substitute |
| Default5s/minimum1s/floor interval, optional first wait | Existing paused-time tests plus actual frozen schedule differential |
| Optional deadline, zero/negative expiry, remaining-time sleep cap | Frozen poll timestamps/elapsed/outcome differential |
| pending, complete, failed | Native and frozen exact result/error messages |
| slow_down server finite positive interval, otherwise5s increment | Native default/server interval tests; frozen schedule and slow-down timeout differential |
| Abort before poll, abort during sleep; cancellation ignores caller reason | Existing in-flight abort test and frozen pre-abort differential |
| Poll promise rejects | Compile RED for missing Result callback; native exact TypeError carrier GREEN; frozen rejected-fetch differential |
| Timeout with/without slow-down | Exact frozen messages, elapsed time and poll count compared |

Current11native tests (one contains11source scenarios). `/tmp/ai3-device-native-green.log`, `/tmp/ai3-device-differential.log`. The source oracle imports the unmodified module and replaces only Date.now/setTimeout with deterministic single-sleeper clock progression. Native uses Tokio paused time. No provider or live network.

Removed source-absent OAuthDeviceCodeError enum in favor of the shared CallbackError; failed-status messages retain JsString and arbitrary callback rejection identity is preserved. Copilot/Codex poll closures were mechanically adapted to Result; their own transport/error policies remain with those rows.

GAP: wall-clock drift versus monotonic time, timer/microtask races, nonfinite/overflow durations and dropped/cold futures follow the approved runtime gap policy. No source bug or required ordinary branch remains open.
