# components/custom-entry.ts

Source: `coding-agent/src/modes/interactive/components/custom-entry.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/custom_entry.rs`

Tests: `interactive_components_c.rs::custom_entry_*`, differential `customEntry`/`customEntryEmpty`.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `CustomEntryComponent` | `CustomEntryComponent` | behavioral + differential |
| `hasContent` | `has_content` | both differential cases |
| `setExpanded` rebuild only on change | `set_expanded` | differential expanded/false |
| `invalidate` rebuild | `Component::invalidate` | rebuild path |
| renderer returns a component -> spacer + component | `rebuild` | differential |
| renderer returns `undefined` -> no content | `Option` -> early return | `customEntryEmpty` |

## Gaps

- The source catches renderer exceptions and renders a themed error box. Rust renderers
  return `Option<ComponentRef>` and cannot throw; a panicking renderer is a bug, so the
  catch path is not representable (typed API exclusion).

## Discrepancies

None found.

## Q18 native render lines (2026-10-22)

Component render output is Vec<JsString>; entry data and renderer output already
use the native authority. raw_leaf_components checks raw payload in collapsed
and expanded renderer output. Written but unexecuted while coding compilation
is blocked. No common-pin whole-module parity claim.

### re-pin gap: lifecycle contract absent (frozen 4f9e6f8a4 custom-entry.ts)

Source `packages/coding-agent/src/modes/interactive/components/custom-entry.ts`:
options/requestRender/sessionId at lines12–22 and constructor32; disposal58–61;
EntryRenderOptions requestRender/sessionId/registerCleanup67–74; renderer-error
fallback76–86; replacement-before-cleanup ordering89–96; cleanup/component
disposal101–109. Rust lacks these contracts. Parent explicitly assigned this as
a separate RED-first re-pin slice after library compilation, not a Q18 fix or
approved semantic parity deviation.
