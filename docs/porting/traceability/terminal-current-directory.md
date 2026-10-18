# coding-agent terminal-current-directory traceability

Pinned source: `packages/coding-agent/src/utils/terminal-current-directory.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `26006` executes the real pinned module across 12 absolute cwd cases including spaces, percent signs, Unicode, punctuation, trailing slashes, duplicate slashes, and dot segments. Focused tests: `crates/pi-coding-agent/tests/terminal_current_directory.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `terminal-current-directory.ts:9-12` | Emit `ESC ] 7 ; file://{hostname}{path} ESC \` | `utils::terminal_current_directory::format_terminal_current_directory_sequence`; focused and differential cases |
| `terminal-current-directory.ts:10` | Convert cwd with `pathToFileURL`, resolving relative paths and dot segments | `resolve_absolute` + lexical normalization; differential dot-segment and duplicate-slash cases |
| `terminal-current-directory.ts:11` | Set the URL hostname to `os.hostname()` | `gethostname::gethostname`; focused hostname assertion |
| Node `pathToFileURL` encoding | Percent-encode all bytes except alphanumerics and `-._/:@!$&'()*+,;=`; encode `~` | `encode_file_url_path`; differential encoding corpus |

## Discrepancies

- None observed. A trailing separator is preserved for non-root paths, matching `pathToFileURL`.

## Proof inventory

- Focused cases: 4.
- Differential request: `26006`, 12 cases.
