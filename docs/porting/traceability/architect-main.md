# architect/main.ts

Frozen `4f9e6f8a4`; Rust `src/architect/main.rs`.

| Export / group | Proof |
|---|---|
| rules/tool constants, settings, broadcast gate, snapshots | `architect` behavior tests; frozen literals compared |
| console queue/server, stop handler, interval, service loop | `architect` FIFO/wake/console lifecycle tests; no live resident service |
| multi-agent store, targeted request completion, cycle/renewal | `differential_architect_request_loop` invokes actual frozen observer/cycle/completion and compares DB/session outputs |
| `openArchitectSession` | `q16_resident_open` selection and pruning differential; old inline discovery removed |
| `runArchitectService` | Compiled session/extension/model/observer wiring only |

Detailed legacy branch mapping: [architect aggregate](architect.md). Whole-mode platform/process orchestration is not differentially proven. [Slice proof](../q16-supervisor-runtime-proof.md).
