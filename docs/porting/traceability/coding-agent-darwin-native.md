# Darwin process addon

Frozen `4f9e6f8a4`; classified under the macOS addon/platform gap in AGENTS.md and parent instruction2026-10-25.

| Source | Status | Boundary |
| --- | --- | --- |
| `coding-agent/native/darwin/build.mjs` | gap | macOS xcrun/SDK Mach-O/N-API prebuild compilation/check/copy; no Darwin addon packaging on this Linux Rust target. |
| `coding-agent/native/darwin/src/process-info.c` | gap | macOS libproc/N-API process birth marker/group/state inspection; Linux native process inspection remains covered in its own module. |
