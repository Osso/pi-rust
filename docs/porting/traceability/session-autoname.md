# session-autoname traceability

Source: `coding-agent/extensions/session-autoname/src/index.ts` at authorized
`c248b0067c9fc3fa438f967f826c222875c54834` (2026-10-20 deviation;
[reference lock](../session-name-source-lock.json)). Rust:
`crates/pi-coding-agent/src/bundled_extensions/session_autoname.rs`.

| Source symbol / branch | Rust | Proof in `tests/session_autoname.rs` |
|---|---|---|
| default factory, agent-end/name-change/shutdown hooks | `session_autoname_extension` | real extension runner in all async cases; `main_process::extensions_command_lists_the_ported_first_party_extensions` proves CLI registration |
| `decideAutonameTurn`, real vs extension user message, continuation carry | `decide_autoname_turn` | `turn_decision_tracks_pending_real_user_turns_across_continuations`, extension-only/history case |
| eligibility: TUI/RPC, persisted file, not child, no stored name state | `can_autoname_session` | explicit clear/manual name/history cases |
| first real user plus completed assistant text; bounded transcript | `find_first_exchange`, `bound_source_text` | first real-user exchange and historical JSONL cases |
| background generation, at-most-once attempt, metadata recheck | `handle_agent_end`, `generate_and_set_title` | successful naming and explicit clear/manual name cases |
| auth, 15s cancellation, retries1/delay cap3000ms/tokens64 | `request_generated_title` | faux-provider integration; timeout/cancellation tests below |
| stop reasons, title error text | `read_generated_title` | `generated_titles_map_stop_reasons_to_outcomes` |
| title prompt/context | `build_title_context` | `title_context_bounds_the_transcript_and_labels_missing_responses` |
| normalization and 80-unit title limit | `normalize_title`, `truncate_title` | normalization/word-boundary/JS-whitespace tests |

Uses the AgentSession's shared manager from `ExtensionContext::session_manager()`.
No external service is contacted in tests; native faux providers exercise the same
request path. JS trim/whitespace/UTF-16 utilities are shared through `pi-js-compat`.

## Explicit gaps

Rust UTF-8 strings cannot represent a lone surrogate produced by JS slicing inside
a supplementary character. Whole-character UTF-16 boundaries are covered; that
split-surrogate output case remains a gap, not an approved normalization.
