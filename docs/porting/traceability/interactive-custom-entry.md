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
