# Extension examples J — forms, RPC, autocomplete

Pinned source: `532f7d12` under `packages/coding-agent/examples/extensions/`.
Native examples are embeddable `#[path]` modules, registered explicitly through
`ExtensionRegistry`; none loads by default. No dependencies added.

| Source | Native entrypoint | Proof / blocker |
| --- | --- | --- |
| `github-issue-autocomplete.ts` | `github_issue_autocomplete_extension` | `example_github_issue_autocomplete`: ordinary-data differential; native U+FFFD boundary regression under approved exclusion below. |
| `question.ts` | Not shipped | Todo: dedicated form slice; host dependency resolved |
| `questionnaire.ts` | Not shipped | Todo: dedicated form slice; host dependency resolved |
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
- Typed issue records exclude invalid JavaScript value types per compatibility policy.

### Approved String-typed fuzzy boundary exclusion

[Compatibility policy](../../specs/compatibility.md#out-of-scope), line 177:
“Lone-surrogate input supplied directly to existing `String`-typed core APIs”.
Raw JSON escapes remain representable and are decoded by `pi_js_compat::js_json`.
Once decoded issue titles enter pi-tui fuzzy's existing `Vec<String>` candidate
API (`crates/pi-tui/src/fuzzy.rs:141`), lone surrogates project to U+FFFD.
Main-thread classification approved this boundary exclusion on 2026-10-20;
it does not exclude raw JSON parsing generally or claim lossless fuzzy parity.

`decoded_lone_surrogates_project_to_replacement_at_string_fuzzy_boundary`
asserts raw JSON containing `\ud800` and `\udfff` decodes to a native issue
whose title contains two U+FFFDs, and query `�` returns issue #77 with its complete
native item fields. TS retains the lone surrogates and delegates that query;
this specific difference is excluded, not differentially asserted.
All ordinary JSON, Unicode, success/error, caching, completion and delegation
fixtures remain in the full-result TS comparison. Historical RED logs 13–14
record the now-classified boundary difference, not an open implementation blocker.

## Forms deferred (not partial implementations)

Both pinned factories instantiate `new Editor(tui, editorTheme)`:
`question.ts:89`, `questionnaire.ts:130`. The original blocker was missing
`ExtensionUiTui` editor-host/fixed-cell and focus support. Master `1e27e41`
resolves that dependency; see [editor-host traceability](extension-examples-editors.md).
Per the final integration decision, both forms remain `todo` for their own slice.
This branch ships no form implementation, failing/ignored stub tests, form renders,
key-sequence results, renderer proofs, or mutation claims.

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
  Historical proof at 9fb0ead; current ordinary-data and boundary proofs follow in the ledger.
- RPC prefill notification mutation: targeted test failed (log 12), reverted.
  RPC source remains the GREEN log-10 implementation (formatting only afterward).
- Form examples have no implementation/tests/mutations; their host dependency is resolved, and porting is deferred.
- Final integration merges master only into this branch; no push, core API edits,
  live state, GitHub network calls, agents or model CLIs. Proof scope and exact
  revisions are recorded in the ledger.

## T3 delegation proof (2026-10-20)

`example_github_issue_autocomplete` fallback encodes forwarded lines, nonzero line/cursor, force, and abort state into its returned prefix. File completion has positive/negative argument-dependent cases. Native UI columns are UTF-8 byte offsets; oracle fallback prefix projects UTF-16 columns to bytes for existing Unicode cases. New forwarding fixtures are ASCII. Passing line 999 to fallback and passing empty file-completion lines each fail independently.
