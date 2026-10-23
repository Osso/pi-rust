# Ls tool

Match `coding-agent/src/core/tools/ls.ts` at immutable4f9e6f8a4. [Traceability](../porting/traceability/ls.md).

## Numeric limits
- [x] Retain optional binary64 limits, including fractions, signed zero, NaN and infinities; absence uses500.
- [x] Filesystem listing compares successful entry count directly to the number before the next stat; skipped stat failures do not count. Empty results return the frozen empty-directory result without a notice.
- [x] RTK listing applies Array.slice conversion, including negative relative indices and NaN→0; its entry-limit flag independently compares line count to the original number.
- [x] Preserve original numeric details and JavaScript spelling of limit/doubled-limit notices; JSON wire serializes non-finite numbers as null.
- [x] Use the shared [slice-index authority](js-slice-index.md), not integer coercion/defaulting.

## Remaining module cycle
- [ ] Exact local/injected IO and error ordering.
- [x] [First-winner abort observer](tool-promise-observer.md) rejects promptly without cancelling retained producer work or skipping later operations.
- [ ] RTK process validation, stdout/stderr/maxBuffer and missing-executable behavior.
- [ ] Stateful call/result rendering, output/warnings, symbol map and current-pin coverage closure.

## Proof
`differential_ls_numbers`:17 actual frozen injected-operation cases plus17 real fake-RTK process cases. Exact traces prove limit checks happen before stat and enough entries remain for a second page. `ls_tool` covers ordinary sorting, dotfiles, directories, limits and byte truncation.

No new dependency or numeric deviation.
