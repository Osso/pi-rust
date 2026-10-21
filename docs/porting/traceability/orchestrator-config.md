# Orchestrator config

Frozen: `packages/orchestrator/src/config.ts` at [common pin](../source-lock.json).

| Surface | Rust | Proof |
| --- | --- | --- |
| Whole module | `crates/pi-orchestrator/src/config.rs` | `foundation::config_precedence_and_empty_overrides` |

Bun virtual filesystem detection is false in the native port; package metadata uses frozen package version.

Status: initial focused proof; complete branch matrix pending. Coverage remains todo until final evidence.

`frozen_config_path_precedence_and_normalization_matrix` covers default/empty/relative/dot-segment/explicit-override and all path accessors against frozen TS; Rust draft normalization failed first and is corrected.

`config_no_home_matches_frozen_os_homedir`: a subprocess with HOME absent compares the actual OS homedir path against frozen config. RED exposed the draft empty-HOME assumption; native now uses OS home lookup matching Node. No host state is read/written beyond sandbox-visible account metadata.
