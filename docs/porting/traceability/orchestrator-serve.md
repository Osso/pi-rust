# Orchestrator serve

Source: `packages/orchestrator/src/serve.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/serve.rs`.
Status: **partial**. [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| serve startup; server/recovery/Radius setup | native start startup-failure cleanup, frozen startup logs and source SIGTERM exit-0 socket cleanup: supervisor |
| shutdown promise/idempotence; uncaughtException/unhandledRejection mapping | native idempotent cleanup; frozen malformed-child exit 1 and native fatal-channel Err, disposed pending send, removed instances/socket: supervisor |
| signal/Radius-enabled boundary | native signal handler and enabled logs implemented; direct embedding-process normal signal and Radius-enabled full serve matrices unproven |

## Explicit proof gaps
- Direct native embedding-process SIGINT/SIGTERM and Radius-enabled full serve startup/shutdown matrices remain unproven.

Native UTF-16 integration: [lossless orchestrator proof](lossless-orchestrator.md).

Current re-pin RED `/tmp/runtime-services-red-serve-mkdir.log`: a regular file at the configured directory leaked Rust OS text instead of frozen EEXIST mkdir text. The startup directory operation now uses the shared Node filesystem authority. Actual-source exported serve failure is compared with native CLI embedding stderr/message and exit1; the embedding flushes its asynchronous writers before process::exit. GREEN pending at commit. Normal native/frozen CLI SIGINT/SIGTERM startup, live list IPC and socket cleanup proof passed at 85a190062 (`/tmp/runtime-services-cli-serve-process.log`).
