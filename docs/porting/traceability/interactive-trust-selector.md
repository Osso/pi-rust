# trust-selector.ts

Source: `coding-agent/src/modes/interactive/components/trust-selector.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/trust_selector.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `getProjectTrustOptions(cwd)` | `trust_manager::get_project_trust_options` | differential (`trust`, cwd `/work/project`) |
| no saved decision → `none` | `format_decision` | differential `savedDecision: null` |
| trusted/untrusted label | `decision_label` | differential |
| inherited decision text | `format_decision` path branch | `interactive_components::trust_selector_renders_saved_decision_variants` |
| preselect saved option | `is_saved_option` + `position` | differential saved entries |
| list render, checkmark, prefix | `update_list` | differential lines |
| up/down, confirm, cancel | `handle_input` | differential input sequences |

Discrepancy (faithful): `ProjectTrustDecision::Unknown` corresponds to `null` in `trust.json`; JS treats it as falsy in `decision ? "trusted" : "untrusted"`, so the port renders `untrusted`.
