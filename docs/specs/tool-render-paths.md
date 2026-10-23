# Tool-rendered paths

- Shortening uses frozen Node `os.homedir()`, including HOME unset passwd lookup and explicit lookup failure; never substitute an empty home directory.
- `shortenPath` and `renderToolPath` preserve the shared native SystemError. Tool renderer callbacks propagate its message to the existing renderer-error boundary; ToolExecutionComponent retains frozen catch behavior.
- Invalid/empty tool paths retain frozen invalid-argument/ellipsis behavior without a home lookup.
- [Traceability and process-level differential proof](../porting/traceability/render-utils.md#q5-current-pin-homedir-boundary-2026-10-23).
