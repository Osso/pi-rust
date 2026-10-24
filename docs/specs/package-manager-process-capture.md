# Package-manager process output capture

Frozen authority: `core/package-manager.ts:2544–2610` at common pin4f9e6f8a4.

- Async capture decodes each delivered data chunk independently as Node UTF-8, concatenates text, and returns stdout with JS whitespace trimming on success. It does not use a streaming decoder across chunks. Stderr is ignored on success.
- Nonzero exit/signal errors retain untrimmed stderr when nonempty, otherwise stdout, with the frozen command/argv/status message.
- A configured timeout remains active through child exit and complete stdout/stderr close. An exited child whose descendants retain inherited pipes can therefore time out. Timeout takes precedence over exit status. SIGTERM is requested once; capture still drains until close, retaining prior decoded text.
- Sync capture decodes whole outputs, prefers nonempty stdout then stderr on success, and trims JS whitespace. Failure prefers nonempty stderr then stdout, with the frozen error wrapper.
- Source strings/error messages remain UTF-16 exact. IO failures must not silently become empty output.

Approved runtime gap: native OS read partitions can differ from libuv; stateless per-delivered-chunk decoding remains required. [Traceability/proof](../porting/traceability/package-manager.md).

Spawn validation, inherited stdio and environment behavior are separate remaining audit branches, not established by this output-capture contract.
