# `components/scoped-models-selector.ts`

Authority: whole 359-line [common-pin](../source-lock.json) module. Rust: `crates/pi-coding-agent-ui/src/modes/interactive/components/scoped_models_selector.rs`. [Contract](../../specs/scoped-models-selector.md).

| Export / helper / branch | Native mapping | Behavioral proof |
|---|---|---|
| ModelsConfig / ModelsCallbacks, constructor | owned available models and configured IDs; retained typed Input/list/footer children | null/subset/empty/duplicate/missing-ID configurations; complete source/native frames |
| source Map keys and available-ID array | HashMap lookup retains last duplicate model; ordered Vec retains every available ID | duplicate key with first/last names; source shows last name for both rows |
| isEnabled / toggle / getSortedIds | exact configured membership/order, missing IDs filtered from visible items only | mixed available/missing/duplicate configured IDs and callback payloads |
| enableAll / clearAll | literal null/filter/target semantics, including frozen length-only collapse | all/none/subset/provider/search mutations plus prior scoped-subset differential |
| move / reorder | configured-order swap, disabled/bounds no-op; signed visible index | ordinary moves, boundary no-ops and filtered-underflow caught/recovery/process sequences |
| buildItems / getFooterText | only existing model rows; configured count includes missing IDs; dirty warning and save hints | exact full frames, empty list, missing IDs, mutation callbacks and persist failures |
| refresh / getSearchInput | shared typed Input is sole query authority; getter returns same live handle | external setter then Ctrl+C, external setter then toggle; raw search and missing IDs |
| updateList | eight-row window, wraparound, provider/status labels and selected name | 12 available models, 15 downs at width24; full source/native output and existing default-theme cases |
| focused | stored flag propagated to actual Input | cursor-marker frames and Input focused state, both directions |
| handleInput | shared navigation/reorder/toggle/all/clear/provider/save/cancel registry; independent Ctrl+C; remaining input forwarded | custom cancel binding, default actions, raw U+D800 input, empty list |
| callback copies and failures | independent EnabledIds passed synchronously; no component-side awaiting/catching | callbacks mutate received lists; subsequent frames/events unchanged; change/persist/cancel throws preserve source state and diagnostic |
| inherited Container / invalidate | real outer Container with live children and typed handles, Deref/capability delegation | child frames, raw appended child, visible invalidation count, removal/clear and detached navigation |

## DISCREPANCIES — retained frozen behavior

**Length-only enable-all collapse:** source :32–39 returns null when the resulting configured list length equals the available-model count, without checking membership. Missing/duplicate IDs can therefore clear the explicit filter early. Preserved under parent authorization 2026-10-22; existing scoped-subset RED→GREEN remains proof, and current mutation sequences retain it.

**Filtered reorder underflow:** enabled `[provider/a, provider/b]`, search `Beta`, then reorder-up moves b in the configured list and decrements the selected visible index from0 to-1. `refresh` clamps only the upper bound. `updateList` clears/rebuilds rows, adds its spacer, then reads `selected.model.name` with no selected item. The frozen TypeError is `Cannot read properties of undefined (reading 'model')`. Native retains signed state and throws through the shared uncaught carrier at the same partial-update point, before footer refresh/change notification. No clamp or repair. A caught sequence compares the partial frame and subsequent down-arrow recovery; actual source/native subprocesses both emit the same diagnostic, exit1, and omit callback/continuation markers. JS stacks are omitted under the approved rule, not fabricated.

## Owned values and consumer audit

Frozen interactive-mode.ts:6154–6186 refreshes before construction; change callbacks copy enabled IDs and update session scope, persist writes settings, cancel closes the selector. It does not mutate selector model records or the supplied configuration afterward. No frozen built-in calls `getSearchInput`; nevertheless its public live-mutation behavior is preserved and proved. Callback argument copies are explicitly required by source, not optional alias projections. Native callback closures own their async scheduling; the component invokes them without awaiting, as source does. No generic backend or Q15-owned file is changed in this checkpoint.

## Old-only refresh removal

Frozen config has no refresh status and the component has no updateModels. Earlier Q18 removal deleted native post-mount refresh requests, timeout/status/disposal code; no alternate path is restored. The prior full-mode session/persist regression remains evidence for that bounded removal, not whole-mode parity. Q15 owns current pre-mount model-registry integration.

## Proof ledger

Artifacts `/tmp/pi-rust-repin-lossless-interactive/`:

- `test-scoped-models-selector-red-3.log`: five isolated real differences (duplicate last-write, live query, cancel registry, inherited children, filtered TypeError). RED1/2 were masked by source-oracle setup using a different TUI singleton; not accepted parity evidence. Source harness now imports pinned TUI by its absolute source path.
- `test-scoped-models-selector-green-4.log`: 2/2; 16 state sequences with full UTF-16 frames, shared query/focus/children, callbacks and caught errors; actual uncaught source/native process boundary. GREEN1 was only an isize-format compile error; GREEN2/3 were narrower development proofs superseded by expanded test coverage, not production changes.
- `test-scoped-models-selector-existing-green-1.log`: 1/1, four unchanged default-theme source cases.
- `test-scoped-models-selector-raw-green-1.log`: 1/1, raw missing-ID/search regression.

Four focused tests cover this whole component checkpoint. Parent owns final readability/clippy/independent gates; no whole interactive-mode/Q13 completion is implied.
