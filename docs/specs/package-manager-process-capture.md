# Package-manager process output capture

Frozen authority: `core/package-manager.ts:2544–2610` at common pin4f9e6f8a4.

- Async capture decodes each delivered data chunk independently as Node UTF-8, concatenates text, and returns stdout with JS whitespace trimming on success. It does not use a streaming decoder across chunks. Stderr is ignored on success.
- Nonzero exit/signal errors retain untrimmed stderr when nonempty, otherwise stdout, with the frozen command/argv/status message.
- A configured timeout remains active through child exit and complete stdout/stderr close. An exited child whose descendants retain inherited pipes can therefore time out. Timeout takes precedence over exit status. SIGTERM is requested once; capture still drains until close, retaining prior decoded text.
- Sync capture decodes whole outputs, prefers nonempty stdout then stderr on success, and trims JS whitespace. Failure prefers nonempty stderr then stdout, with the frozen error wrapper.
- Source strings/error messages remain UTF-16 exact. IO failures must not silently become empty output.

Approved runtime gap: native OS read partitions can differ from libuv; stateless per-delivered-chunk decoding remains required. [Traceability/proof](../porting/traceability/package-manager.md).

- All three launch paths forward the shared modeled process environment. Async capture overlays supplied env values; values cross Node's UTF-8 OS boundary only at launch. On Linux, an empty modeled environment restores `/proc/self/environ` entries split at the first equals sign, ignoring empty names; a read failure retains the empty environment (frozen `getEnv`:6–23).
- Ordinary async commands inherit stdin/stdout/stderr. During stdout takeover, stdin is ignored and both outputs duplicate inherited stderr, including its shared file offset (frozen :2515–2535).
- Approved runtime gap: frozen :2599 specifies no `maxBuffer`. Node's implicit spawnSync output limit is not reproduced under the 2026-10-23 runtime-oddity policy; native sync capture retains complete output.

- Bounded async worker groups reject on the first worker failure without awaiting or cancelling surviving workers. A failed worker consumes no more queued tasks; survivors continue the queue after rejection. Successful result arrays retain input order. Parallel user/project npm updates and git worker groups use the same reject-fast lifetime semantics (frozen :1083–1114,1214,1608–1629).
- Owned native worker handles retain the manager's shared callback/cache state after the initiating future rejects. Independent worker scheduling is native, not a JS microtask emulation.

[Spawn input/error behavior](package-manager-spawn.md) has its own contract.
