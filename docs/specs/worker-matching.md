# Raw worker matching (Q18)

## Goal

Port worker matching over arbitrary JavaScript UTF-16 units: literal lowercase/includes, legacy RegExp, and the frozen worker glob function. Scope includes the shared minimatch AST/brace/class engine and factoring validation's existing regex adapter; excludes other providers, TUI, and filesystem behavior. Complete when targeted frozen-source differentials and matcher regression tests pass in the mandatory sandbox, formatted commits exist, and limits are recorded. No broad acceptance gates in this bounded slice.

## Requirements

- Preserve unmatched high/low units, paired units, and distinct replacement/private-use characters.
- Regex legacy mode consumes UCS-2 units; Unicode mode is explicit and preserves the existing validation behavior.
- Glob parsing, AST storage, brace alternatives and classes use raw units, not surrogate encodings or scalar rejection.
- Actual immutable `fs-worker.cjs` governs worker semantics. It uses `path.matchesGlob` when available and a simpler explicit source fallback, not the handoff's assumed default minimatch call.
- Regex compile failures remain errors; raw pattern units must survive error construction.

## Proof

See [traceability](../porting/traceability/worker-matching.md). This slice does not approve scalar restrictions elsewhere.
