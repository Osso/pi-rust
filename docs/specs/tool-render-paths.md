# Tool-rendered paths

- Shortening and tilde path resolution use frozen Node `os.homedir()`, including HOME unset passwd lookup and explicit lookup failure; never substitute an empty home directory or panic around renderer catch. Path canonicalization alone retains its frozen explicit catch-and-return-original behavior.
- `shortenPath` exposes the shared native SystemError. Path linking/rendering propagate lookup and resolution diagnostics through PathError to existing renderer HandlerError callbacks; ToolExecutionComponent retains frozen catch behavior.
- Invalid/empty tool paths retain frozen invalid-argument/ellipsis behavior without a home lookup.
- [Traceability and process-level differential proof](../porting/traceability/render-utils.md#q5-current-pin-homedir-boundary-2026-10-23).
