# Read-only terminal export

`--export-terminal <session-file-or-id> --export-columns <positive-safe-integer>` writes LF-terminated ANSI history to stdout without starting a terminal, agent, extensions, settings writer or control DB. Only `--session-dir` and `--offline` can accompany it. Invalid arguments exit 1; an unmatched session ID exits 4. External session aliases fail explicitly.

Render the loaded compaction-aware session view with normal collapsed components, disabled images and no bash elapsed label. Remove saved terminal control protocols and binary controls before rendering; preserve only renderer SGR styles in output, reset every line and end with LF. Use global settings read-only for theme/session directory, preserving CLI/environment/settings precedence.

The Rust renderer lives in `pi-coding-agent-ui::export_terminal` to use existing widgets without introducing a crate dependency cycle. `pi-coding-agent-interactive` re-exports it for CLI startup. [Traceability](../porting/traceability/export-terminal.md).
