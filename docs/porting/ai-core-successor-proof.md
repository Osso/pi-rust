# AI core successor checkpoint — 2026-10-25

## Scope and result

All seven remaining handoff rows are closed against frozen `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`: `ai/src/types.ts`, `ai/src/oauth.ts`, and `utils/oauth/{index,types,anthropic,github-copilot,openai-codex}.ts`. This uses the approved lighter proof: whole-module source/native audit plus existing tests; each observed discrepancy has a native regression, not a new frozen differential/matrix. The six predecessor-closed rows (compat, images-models, Faux, retry, validation, device-code) remain done.

Code revision verified: `7d5da84c3752ade33a3c8bf6d672ea7e95415111`, based on merged parent `50891afc5`; subsequent checkpoint edits are documentation-only. Initial parent `7eddb62a3` was merged before work. The later parent merge was a fast-forward because the train had already consumed earlier successor commits.

Current inventory: **686 files / 186,679 frozen lines**, **648 done /25 gap /7 partial /6 todo**. Exactly seven lane status rows changed; frozen paths/line counts and predecessor-closed statuses are unchanged. Source discrepancies and runtime exclusions are in each module trace, not in a new proof matrix.

## Capability coverage

[Public types](traceability/ai-types.md): complete source union/field/option/content/message/event/model/routing export audit; historical fields stay lossless on existing raw authorities. Retired unused Baseten enum and ModelCostTier/typed tiers, retaining unknown cost properties and original order. Existing native/frozen record witness passes. No source-absent deferred/constrained/sampling contract remains.

[OAuth entry/registry/types](traceability/oauth-registry.md): all direct helpers/constants/builtin accessors, ordered replacement/restore/reset, lookup/refresh/API-key branches, full callback optionality/native values and exact Result failures. Public compile RED identified absent exports and notifier Result surface. Existing credential authority and fixed/order metadata are unchanged.

[Anthropic](traceability/anthropic.md), [Copilot](traceability/auth-oauth-github-copilot.md), [Codex](traceability/openai-codex.md): complete source flow/control audit, normal success/error/cancellation/manual/callback/refresh branches; existing frozen HTTP proof retained. Twenty native edge regressions fix parser truthiness/duplicates/hash segments, manual/optional callback handling, listener lifetime/bind/query errors, rejection identity, response-side catches, SyntaxError/cause formatting, expiry clock timing, Copilot signal/body/default URL behavior. Codex's frozen onAuth-before-finally listener leak is preserved literally and proven in isolated processes.

Uncovered/disabled only by standing approval: adversarial/wrong-typed or exotic inputs, runtime/browser/Node readiness, numeric extremes and Node/V8/libuv scheduling/fault oddities. Existing lossless carriers remain intact. No live provider/network use beyond fake fetch and loopback; no agent/model, push or deployment.

## Proof ledger

All commands below ran through Pyrun argv-style helpers in this worktree. Runtime tests used the configured mandatory sandbox runner. Production proof scope is the exact code at `7d5da84c3`; subsequent documentation does not invalidate these results.

- `cargo test -j3 -p pi-ai --test suite_core_2 oauth -- --test-threads=1`: **57 passed**, `/tmp/ai-successor-oauth-final.log`; native flows, all20 edge regressions, legacy callbacks, public exports and existing source-backed flow fixtures.
- `cargo test -j3 -p pi-ai --test suite_core_2 types:: -- --test-threads=1`: **26 passed**, `/tmp/ai-successor-types-final.log`; includes the two public OAuth tests also selected above, complete frozen record witness and source-absent cost metadata regression.
- `cargo test -j3 -p pi-ai --test suite_core_1 oauth -- --test-threads=1`: **15 passed**, `/tmp/ai-successor-oauth-wire-final.log`; retained actual frozen OAuth HTTP/credential/page and auth-helper fixtures.
- `cargo test -j3 -p pi-ai --test suite_core_1 differential_anthropic_timeout -- --test-threads=1`: **3 passed**, `/tmp/ai-successor-oauth-deadlines-final.log`; independent30s/5s deadline identity and source error formatting.
- `cargo test -j3 -p pi-ai --test anthropic_oauth --test oauth_registry --test oauth_abort_wrapper --test auth_types_contract -- --test-threads=1`: **6+3+1+5 passed**, `/tmp/ai-successor-oauth-standalone-final.log`; exact raw/default/custom causes, actual-source notifier errors and listener-leak process boundary, native wire/header/form/token checks.

Additional bounded proof:

- `cargo test -j3 -p pi-coding-agent --test auth_codex_credential_order -- --test-threads=1`: **3 passed**, `/tmp/ai-successor-codex-persistence-final.log`; unchanged existing seven frozen wrapper scenarios preserve email precedence/native units and exact auth.json shape/key order, plus interleaved native credential round-trip. credentialsFromToken/key-order code was not reimplemented.
- `cargo test -j3 -p pi-ai --test suite_modes calculates_cost_from_base_rates_even_with_old_tier_metadata -- --test-threads=1`: **1 passed**, `/tmp/ai-successor-cost-unknown-final.log`; historical cost metadata remains inert and preserved.
- `cargo clippy -j3 -p pi-ai --all-targets --message-format=json`: **exit0; zero changed-line warnings**. `/tmp/ai-successor-clippy.jsonl`, `clippy-changed.json`; retained pre-existing multiple-module fixture warnings are not suppressed.
- `cargo clippy -j3 -p pi-coding-agent --all-targets --message-format=json`: **exit0; zero AI-lane-owned changed-line warnings**. `/tmp/ai-successor-callers-clippy.jsonl`, `callers-clippy-changed.json`. Five diagnostics intersect changes inherited from the parent: interactive_mode.rs8662 await_holding_lock and8795/8916 collapsible_if, package_manager_cli.rs1240 collapsible_if twice (lib/bin). None is in AI callback/caller edits; they are not fixed or hidden by this lane.
- `cargo check -j3 --workspace --all-targets`: **exit0** on each coherent compilable successor tip and latest full code (`/tmp/ai-successor-check-flow-final.log`); final documentation checkpoint rechecks before commit under main's updated ordering. `cargo fmt --check` and `python3 scripts/source-inventory.py` pass before every checkpoint. Final logs use the `/tmp/ai-successor-final-*` prefix.

## RED provenance

`/tmp/ai-successor-edges-red.log`: seven initial native behavior failures. `/tmp/ai-successor-clock-signal-red.log`: pre-response expiry and unwanted signal. `/tmp/ai-successor-public-red.log`: genuine missing public names/notifier result compile failure. `/tmp/ai-successor-body-manual-red.log`: response catch, JSON brand and non-Error manual rejection. `/tmp/ai-successor-types-red-final.log`: unknown cost.tiers wrongly validated as known; earlier two attempts were fixture compilation errors, not behavior proof. `/tmp/ai-successor-callback-red.log`: empty callback code and Node listen diagnostic. `/tmp/ai-successor-copilot-response-red.log`: policy body28reads, JSON brand, empty enterprise endpoint. `/tmp/ai-successor-manual-absent-red.log`: real listener-failure/absent-manual deadlock. `/tmp/ai-successor-listener-order-red.log`: listener remained open during pending token request. `/tmp/ai-successor-branded-cause-red.log`: branded nested cause became [object Object].

## Mechanical fallout and readability

Synchronous legacy OAuth callbacks now return Result: CLI/interactive implementations add Ok(()); two example providers and test fixtures propagate with ?. AuthInteraction has an optional manual bridge solely to preserve legacy missing-vs-pending semantics; modern implementations retain the default required manual prompt. Source-absent cost fields were removed from16 constructors across AI/agent-core/coding-agent fixtures; no caller algorithm changed.

Manual Rust readability audit covered all changed production lines and regressions: no new suppression, lossy conversion, ignore, catch_unwind, dependency or fallback. Small transformation/error helpers are named; callback/bind/body effects remain explicit. Existing long browser orchestration is retained to keep source lifecycle order visible rather than perform an unrelated redesign. The metrics executable is unavailable; this is a manual audit, not a metrics claim. No TypeScript code changed.

Process correction: `de1ebbc38` temporarily omitted constructor fallout and failed the workspace check; `724873981` contains the complete mechanical repair. Main merged that repair and required workspace check **before** subsequent commits. The final flow commit and documentation checkpoint follow that order. No uncompiled patch or active background job remains.

No independent verifier/reviewer was used: the user explicitly prohibited agents and other models. These are direct observed proofs, not a claim of an independent gate or full-workspace test suite.
