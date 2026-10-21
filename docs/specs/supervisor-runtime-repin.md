# Supervisor/runtime frozen re-pin (Q16)

Reference: [common source lock](../porting/source-lock.json), `4f9e6f8a4`.

Supervisor request prompts reconcile cumulative objectives against explicit user instructions and persistent contracts. Assistant plans, prior reviews, summaries and repeated objectives do not authorize constraints. Goal-set and goal-progress requests preserve grounded parent requirements, exclusions and completion criteria, remove unsupported additions, and permit scope narrowing only by explicit user instruction. Goal-progress responses avoid routine task decomposition.

The resident regex delta remains governed by [delta lock](../porting/delta-lock.json). Q14 standalone `--no-supervisor` behavior is outside this slice.
