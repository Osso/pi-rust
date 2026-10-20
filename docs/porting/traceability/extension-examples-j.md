# Extension examples J — forms, RPC, autocomplete

Pinned source: `532f7d12` under `packages/coding-agent/examples/extensions/`.
Native examples are embeddable `#[path]` modules, registered explicitly through
`ExtensionRegistry`; none loads by default. No dependencies added.

| Source | Native entrypoint | Proof / blocker |
| --- | --- | --- |
| `github-issue-autocomplete.ts` | Partial `github_issue_autocomplete_extension` | STOPPED: lossless UTF-16 fuzzy candidate API absent; current targeted test RED (log 14). Ordinary-data differential previously GREEN; not full parity. |
| `question.ts` | Not shipped | Blocked on extension editor host, below |
| `questionnaire.ts` | Not shipped | Blocked on extension editor host, below |
| `rpc-demo.ts` | `rpc_demo_extension` | `example_rpc_demo`: full real-RPC protocol frame comparison |

## GitHub issue autocomplete

- Factory/session-start: git remote resolution (SSH, HTTPS, HTTP, unrelated
  host, non-repository), per-session eager issue load, wrapper installation.
- `getIssues`: one cached promise per session, exact gh arguments, success,
  empty list, command failure with stderr/exit-code detail, malformed JSON;
  load error emitted once per session. Session restart reloads.
- Provider `getSuggestions`: issue token boundaries, whitespace/tab/newline,
  numeric prefix priority, fuzzy matching, Unicode title, top-20 ordering,
  no matches, empty issues, aborted signal, fallback delegation.
- `applyCompletion` and `shouldTriggerFileCompletion`: delegated results compared.
- Test-owned fake `gh` executable records requests and reads deterministic JSON
  responses in the temporary cwd. Real git reads fixture remotes only. All
  descendants execute inside cargo's mandatory test sandbox; no GitHub contact.
- Typed issue records exclude invalid JavaScript value types per compatibility
  policy. Raw JSON escaped lone surrogates are **not** excluded: see blocker.

### STOPPED: lossless fuzzy candidate API

Fixture gh JSON: `[{"number":77,"title":"Lone \ud800 and \udfff","state":"OPEN"}]`.
Query `#�` must delegate to the base provider: TS's raw title contains no U+FFFD.
The partial native port converts raw JSON strings to U+FFFD before matching and
incorrectly returns issue #77. Log 14 compares the complete mismatching results;
log 13 first reproduced serde's rejection of these otherwise-valid JSON escapes.

Required API: fuzzy filtering accepting lossless UTF-16 candidate strings, with
conversion only when formatting `AutocompleteItem` String fields. Existing
`crates/pi-tui/src/fuzzy.rs:141` requires `get_text: impl Fn(&T) -> Vec<String>`;
`fuzzy_match` at line 22 also accepts only `&str`. `js_json::JsString` preserves
the input, but no fuzzy entrypoint accepts it. This batch must not duplicate or
change core fuzzy logic. Implementation is frozen as partial; no current GREEN
or full-parity claim. The failing regression remains enabled, not ignored.
Approved U+FFFD conversion at the final UI boundary is applied to TS item fields
in the shared oracle; converting before fuzzy matching is not that boundary.

## Form blockers (not partial implementations)

Both pinned factories instantiate `new Editor(tui, editorTheme)`:
`question.ts:89`, `questionnaire.ts:130`. Rust `EditorHost` requires
`create_fixed_cell` (`crates/pi-tui/src/components/editor.rs:122-125`).
The available `ExtensionUiTuiBackend`/`ExtensionUiTui` expose rendering, dimensions,
and mode only (`crates/pi-coding-agent/src/core/extensions/types.rs:277-310`),
not fixed-cell construction or an `EditorHost` implementation. The Rust editor
also owns `Rc` state while `ExtensionUiComponent` requires `Send + Sync`
(`types.rs:262`). A core slice owns editor-host/focus work; this batch does not
reimplement an editor or add a competing bridge. Consequently no form renders,
key-sequence results, renderer branches, RED/GREEN or mutation proofs are claimed.

## Proof ledger

Commands and outcomes: `/tmp/pi-rust-ext-examples-j/ledger.md`; numbered full
logs in that directory. Only targeted example tests and formatting are run.

## RPC demo

`example_rpc_demo` runs the real Rust `run_rpc_mode` and pinned TS `runRpcMode`
in isolated children with private in-memory SDK sessions, model runtime without
network, and an explicit native registry / pinned real extension loader.

- `session_start`: startup and new titles, complete widget lines/status, count
  retained across session transition; `turn_start` increments twice, `turn_end`
  reports current count.
- `tool_call`: non-bash, safe bash, rm -r/-rf/--recursive, case-insensitive sudo,
  ASCII word boundaries beside Unicode, JS FEFF whitespace versus U+0085,
  allow/block/cancel and missing UI. No bash command executes: test-only RPC
  `/fixture-event` emits through the real extension runner and serializes its
  complete result as a notification.
- `session_before_switch`: resume, no UI, confirmed and cancelled new-session
  transitions through the real RPC `new_session` command.
- All three public commands run as real RPC prompts: input/editor nonempty,
  empty and cancelled responses, multiline editor counting, editor prefill.
  `get_commands` compares full command metadata.
- Every protocol frame (including queue updates and command responses), its
  complete fields and ordering is compared. Only generated UI request IDs are
  normalized; no stack frames or message text are discarded.

Observed adjacent discrepancy: relative extension path `rpc-demo` gives native
command `sourceInfo.baseDir == ""`, TS `"."` (log 08). This belongs to core source
metadata, outside the allowed example files; the fixture uses the same absolute
extension path `/fixture/rpc-demo` on both sides. No core fix or fallback added.

## Mutation checks and final state

- GitHub top-20 → top-19 mutation: targeted test failed (log 11), reverted.
  Historical proof at 9fb0ead; later surrogate work invalidates GitHub GREEN.
- RPC prefill notification mutation: targeted test failed (log 12), reverted.
  RPC source remains the GREEN log-10 implementation (formatting only afterward).
- Form examples have no implementation/tests/mutations because of missing host API.
- No merge/push, core API edits, live state, GitHub network calls, agents or model
  CLIs. No lint/check/workspace suites. `cargo fmt` and `cargo fmt --check` pass.
