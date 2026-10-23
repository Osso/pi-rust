# Node shell command transport

Match frozen `agent-core/src/harness/env/nodejs.ts` at `4f9e6f8a4`, including shell-path behavior unguarded on POSIX.

## Requirements

- [x] Normalize slashes and apply native JS lowercase before matching drive-letter Windows System32/Sysnative bash.exe paths, including Unicode lowercase mappings.
- [x] Matching paths receive `-s` and command bytes on stdin, without an argv command. All other shells receive `-c` plus the command argument.
- [x] Stdin commands permit embedded NUL and encode native text at the Node UTF-8 boundary. Argv command validation remains unchanged.
- [x] Large stdin writes run concurrently with output collection/process lifetime; source-ignored pipe errors do not become execution failures.
- [x] Child exit closes pending stdin writing even when descendants retain an input descriptor. Output descriptors retain their existing EOF settlement semantics.

## Implementation and proof

[Node environment traceability](../porting/traceability/harness-nodejs-env.md). `repin_node_legacy_wsl` compares actual frozen/native processes, not shell-config construction. It covers16 cases: System32/Sysnative/Unicode drive normalization, ordinary/raw/NUL/large commands, early pipe closure, inherited stdin after exit and nonmatching argv control.

## Exclusions

Windows-platform shell discovery, taskkill and Windows path semantics remain explicit approved gaps in AGENTS.md; the unguarded legacy-shell classifier/transport is not excluded.
