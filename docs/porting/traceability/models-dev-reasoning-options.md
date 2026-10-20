# models-dev-reasoning-options.ts

Source: `packages/ai/scripts/models-dev-reasoning-options.ts` at runtime pin
`532f7d12cd19235081bd4d6e425bde185652d86f` (read-only checkout in
[source lock](../source-lock.json)). Rust public path:
`pi_ai::models_dev_reasoning_options::{ModelsDevReasoningOption, ModelsDevEffort, get_effort_thinking_level_map}`.
Uses existing `pi_ai::types::{ModelThinkingLevel, ThinkingLevelMap}`; map retains
insertion order. No runtime effort selection, catalog, or configuration changes.

## Export and branch coverage

| Source item / case | Rust representation | Behavioral proof |
| --- | --- | --- |
| Exported `ModelsDevReasoningOption` union | Tagged enum: `Toggle`, `Effort`, `BudgetTokens` | `option_union_preserves_wire_values_and_optional_budget_bounds`: all tags, eight effort strings plus null, absent/min/max/both budget bounds, fractional numbers |
| Effort values, excluding ultra | `Vec<Option<ModelsDevEffort>>`; `None` is JSON null | Same public test; ultra and numeric effort rejected |
| `getEffortThinkingLevelMap(options)` | `get_effort_thinking_level_map(&[ModelsDevReasoningOption]) -> Option<ThinkingLevelMap>` | All public tests and pinned-source differential |
| Flatten multiple effort lists; toggle/budget ignored | Collect effort values into membership set | Mixed-options public test; differential reversed/duplicate lists interleaved with toggle/budget/empty effort |
| Empty flattened effort list → undefined | `None` | Empty options/empty effort/toggle/budget public tests and differential |
| Only default/null → undefined | `None` | Unusable-efforts public test and differential subsets/pairs |
| None-only produces map; no inferred off otherwise | `Off` value is `Some("none")` only when verified | `none_only_and_max_only_keep_distinct_off_values`; all differential subsets |
| Off first, then minimal/low/medium/high/xhigh/max; unsupported values null | Existing insertion-ordered map with all seven keys | Exact serialized string assertion; differential exact source `JSON.stringify` bytes |
| Ultra absent, strings never numeric | Fixed source-level table with existing shared level keys | Public exact output/absence assertion and differential |

Differential: `crates/pi-ai/tests/differential_models_dev_reasoning_options.rs`
loads the real pinned module through `oracle/models-dev-reasoning-options-oracle.ts`
and existing `tests/support/oracle.rs` (source HEAD check, process group,
parent watchdog, teardown). 1,110 requests: all 512 subsets of the nine legal
values, each also reversed and duplicated across mixed controls (1,024 total),
81 ordered value pairs across multiple efforts, and five non-effort cases.
Defined maps compare original source JSON strings to Rust JSON strings verbatim:
no sorting, field removal, or output normalization. Undefined is represented by
the oracle envelope's omitted `json` property, distinct from a defined map.

## Explicit exclusions and discrepancies

Native typed API excludes invalid JavaScript runtime inputs: non-array options,
null/malformed option records, missing/non-array effort values, unknown tags,
arbitrary effort strings, numeric efforts, and invalid budget field types.
Budget bounds use `Option<f64>` for TS optional number fields; they never affect
the map. NaN/infinities are not JSON fixtures. No numeric effort translation,
CLI, Node API, generic extensibility, new dependencies, or source discrepancies.
These exclusions follow the repository's approved native typed API boundary.

## Proof ledger

| Command | Revision/scope | Result / validity |
| --- | --- | --- |
| `timeout 900 cargo test -p pi-ai --test models_dev_reasoning_options --test differential_models_dev_reasoning_options` | Base `c732b1e` + new tests, before production code | RED, exit 101: both public imports missing (E0432); `/tmp/claude/reasoning-option-map-red.out` |
| `cargo fmt --all`; `cargo fmt --check` | Slice implementation before commit | PASS, exit 0 each |
| Same focused test command | Committed implementation | Pending GREEN; no broad/check/clippy/type/readability/final gates authorized |
