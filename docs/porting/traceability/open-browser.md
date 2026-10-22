# coding-agent open-browser traceability

Pinned source: `packages/coding-agent/src/utils/open-browser.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential test skipped: invoking the module opens a real browser.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `open-browser.ts:11-16` | darwin → `open`, win32 → `rundll32 url.dll,FileProtocolHandler`, otherwise `xdg-open` | `utils::open_browser::browser_command`; focused `selects_the_platform_launcher` |
| `open-browser.ts:10,22` | `target` string to spawn argv (Node UTF-8, unpaired surrogate → U+FFFD) | `open_browser(&JsString)` encodes via `encode_node_utf8` at the argv boundary |
| `open-browser.ts:10-22` | Never invokes a shell; the target stays one argument | `browser_command` + `Command` argv; focused `keeps_metacharacters_as_a_single_argument` |
| `open-browser.ts:21-23` | Launch is best effort; launcher errors do not crash | `launch_browser_command` ignores spawn errors; focused `launcher_failures_do_not_panic` |

## Discrepancies

- The source calls `unref()` on the detached child. Rust drops the child without waiting; the launcher is not reaped by this process.
- Actual successful browser launch is not exercised in tests.

## Proof inventory

- Focused cases: 3.
