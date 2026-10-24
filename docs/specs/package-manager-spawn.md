# Package-manager spawn inputs and failures

Frozen authority: `core/package-manager.ts:2515–2542,2596–2610`, through Node's POSIX child-process implementation.

- Validate file before argv, argv in index order, then optional cwd and captured-environment entries. Empty file/NUL errors retain Node TypeError branding, code, exact argument/property label and UTF-16 inspection/truncation.
- Async exec failures retain Node's error name/message, negative errno, code and syscall. Deferred errors include the original executable path and argv; immediate syscall failures such as ENOTDIR omit those fields. Preserve original strings, including unmatched units in diagnostics; UTF-8 projection occurs only at the OS call.
- Sync input-validation throws escape unchanged. A returned sync execution error is wrapped in the frozen `Failed to run ...` Error, using its message rather than copying its metadata.
- Environment names use the established scalar String domain; values remain JsString until native encoding. Windows-specific process behavior remains excluded by the recorded platform gap.

[Coverage/proof](../porting/traceability/package-manager.md). Environment forwarding, inherited stdio and synchronous execution limits remain separate audit branches.
