# Executable availability

Reference: frozen `coding-agent/src/utils/executable.ts` at `4f9e6f8a4`.

- Empty commands are unavailable. Explicit paths resolve against the supplied cwd; otherwise use process cwd.
- Search PATH components in order; an empty component uses the selected cwd. A missing/undefined PATH uses `/usr/bin:/bin` on POSIX. Environment key lookup is case-insensitive and takes the first key in property order.
- A candidate must be a regular file, following symlinks, and pass executable access. Filesystem lookup/access exceptions mean unavailable. Errors obtaining process cwd remain explicit failures.
- Keep raw UTF-16 in commands, cwd and environment values until Node-compatible OS encoding. Native OS bytes use shared decoding.

Windows PATHEXT, alternate separators and access rules are the approved Windows-only gap. Typed environment values are string or undefined.

[Proof map](../porting/traceability/executable.md).

The built-in index retains all 19 names and definitions in source order. Its executable-availability factory removes only outline/symbol/references when code-index is unavailable, unless custom code-index operations are supplied. Standalone executable tools have no invented extension context.

AgentSession default runtime construction uses the availability-filtered definitions. Cwd lookup failures propagate through construction and reload as catchable errors; they must not crash the process or replace the previous tool registry. Explicit base-tool overrides retain their existing behavior.
