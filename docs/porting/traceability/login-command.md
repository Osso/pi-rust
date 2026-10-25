# Login command

Frozen `packages/coding-agent/src/cli/login-command.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`; all217 lines audited. Native `src/cli/login_command.rs`.

| Frozen exports / branches | Proof |
| --- | --- |
| `handleLoginCommand`: other/missing/empty provider, help anywhere, unknown flags, browser flag, exit codes | Actual-source callback/flag/file-byte differential; RED `/tmp/settings-models-login-red.log` proves empty provider incorrectly printed unknown-provider instead of usage |
| `runLoginCommand`: unknown provider/list, logging intro, success/failure, prompt lifecycle | Exact stdout/stderr and complete provider id/name order compared with frozen, plus existing native success/failure/native-error fixtures |
| `runLoginWithPrompt`: auth/instructions, device code/expiry, prompt placeholder/manual input, progress, select default/index/id/none/empty options | Actual frozen provider callbacks vs native public AuthStorage login; exact prompts, answers, auth.json bytes and unrelated credential preservation |
| `collectInputLines`, createPromptReader non-TTY: UTF-8 boundary, CRLF splitting, EOF/default answer | Real piped native StdioLoginIo differential; RED `/tmp/settings-models-login-stdio-red.log` shows final bare CR removed from persisted access token. Native now replaces only CRLF pairs, preserving final lone CR |
| `selectOption`: trim/default, parseInt prefix, exact id and unmatched/empty lists | Concrete2suffix/second/blank/bogus/empty-option inputs in the actual-source matrix |
| Browser CLI optional success/failure, Promise.all before saved line, default process argv/exit wrapping | Injected deterministic browser scenarios plus a real private browser-cli executable returning7; URL argv/log, full output and successful saved credentials compared with frozen; no browser launched |
| Default Chalk colors use stdout capability even when writing errors | Real stderr pty/stdout-file differential; RED `/tmp/settings-models-login-color-red.log` proves incorrect native red escapes with redirected stdout |

Gap: Node readline cursor editing/history and independent input/query scheduling are native terminal/runtime oddities (parent approval2026-10-25); prompts, answers, output and persistence remain exact. No live OAuth provider, HTTP request or installed browser is used in these proofs. OS/runtime fault shapes and pathological input types retain the repository parity exclusion.

The oracle imports the actual frozen command, auth storage and OAuth registry. Native process fixtures inject only synthetic provider interactions/HTTP-free browser behavior; protocol fields, output, errors, provider list and complete file strings are not stripped or recursively normalized. The sole pty transform is standard ONLCR applied to expected newline bytes, not ANSI stripping. Source/native focused GREEN and clippy follow the implementation commit in the lane proof ledger.

Current closure proof: a8c36bbd2:9 active tests GREEN (18 actual-source cases + separate real-pty color test), `/tmp/settings-models-login-green.log`; warning-mode root clippy PASS, `/tmp/settings-models-login-clippy.log`, unchanged lints only at118/198. Native public callbacks, exact provider list and auth.json bytes are included; no whole-workspace rerun beyond the explicitly requested5f55fa7f0 gate.
