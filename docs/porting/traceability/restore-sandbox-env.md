# coding-agent restore-sandbox-env traceability

Pinned source: `packages/coding-agent/src/bun/restore-sandbox-env.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Status: **gap** — no Rust implementation.

The module is a workaround for [oven-sh/bun#27802](https://github.com/oven-sh/bun/issues/27802): Bun-compiled binaries observe an empty `process.env` inside sandbox environments, so the entrypoint re-reads `/proc/self/environ` and repopulates `process.env` before any other work. Its only branch conditions are `process.versions.bun` and `Object.keys(process.env).length === 0`.

The Rust port has no Bun runtime, no `process.env` object to repopulate, and no empty-environment defect to repair: environment access goes through `std::env`, which reads the real process environment. Porting the workaround would add a code path that can only run in a runtime the port does not use.

Consequences:

- No `bun/` module exists in `crates/pi-coding-agent`.
- The coverage row records `gap`; nothing depends on this file.
