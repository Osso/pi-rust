# core/runtime-process-darwin.ts — platform gap

Frozen `4f9e6f8a4`, 51 lines. No Rust counterpart. Linux-only runtime-process proof does not cover this module.

| Frozen symbol / branch | State |
|---|---|
| `DarwinProcessInfo` | No native Darwin backend |
| private binding interface/cache/loading | Loads `native/darwin/prebuilds/darwin-{arm64,x64}/process-info.node`; rejects unsupported architectures or absent/non-function `readProcessInfo`; not ported |
| private result validation | Equal PID, positive safe creation marker, nonnegative safe group, R/Z/X states; not ported |
| `readDarwinProcessInfo` | Calls/validates native binding, propagates native errors; not ported |

Classification follows the existing [runtime-process macOS gap](runtime-process.md), not a new approval or parity claim. Linux cannot validate the native addon contract; user/parent review still owns broader platform approval. [Slice proof](../q16-supervisor-runtime-proof.md).
