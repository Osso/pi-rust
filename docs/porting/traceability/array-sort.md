# Pinned V8 Array TimSort

Primary reference: nodejs/node tag `v26.11.1`, `deps/v8/third_party/v8/builtins/array-sort.tq` (1466 lines). [Exact source/PSF license hashes](../v8-array-sort-source-lock.json). Supplied local file was read as data, never executed. Oracle uses actual pinned Node Array.prototype.sort, not the supplied file or a replica.

Rust: `crates/pi-js-compat/src/array_sort.rs::js_array_sort(&mut [T], FnMut(&T,&T)->f64)`.

| V8 statement / branch | Native mapping | Behavioral proof |
|---|---|---|
| user comparator Number/NaN result, :347–360 | f64 callback; `compare_sign` preserves sign and treats NaN/±0 as equal | exact ordered callback/results trace for NaN, infinities, fractions, zero and stateful functions |
| ArrayTimSort `<8` binary-only route, :1416–1431 | `BINARY_INSERTION_THRESHOLD`, binary insertion from0 | lengths0,1,2,7 and boundary8 |
| BinaryInsertionSort, :483–532 | rightmost-equal binary insertion, overlapping shift | short arrays and forced runs, stable duplicate identities |
| CountAndMakeRun/reverse, :552–606 | strictly descending reversal; nonnegative ascending runs | ascending/descending duplicates, alternating32-element runs |
| ComputeMinRunLength, :1152–1179 | threshold64 bit-fold |31,32,63,64,65,127,128,129,255,256,257 and larger |
| pending-run collapse/force collapse, :1184–1268 | same run invariants and merge selection | mixed/unequal runs through2048 elements; exact comparison sequence |
| MergeAt trimming/low-vs-high, :609–674 | shared gallop trim and min-side temp copy | left-short/right-short and two ordered16-win streak patterns |
| GallopLeft/Right, :687–872 | same exponential probes/binary strictness | complete callback-pair trace, both direction/tie paths |
| MergeLow/High and minGallop adaptation, :882–1148 | same straight/gallop loops, early exits including inconsistent comparator zero-run cases |840-case identity-order and exact comparator-trace differential |
| workarray before final copy, :17–180,1272–1414 | sort index permutation; apply cycles only after comparison completes | non-Clone Item fixture; all callback arguments retain original identities |

Dense typed-slice scope is explicit: holes, undefined-property compaction, JS accessors, default string comparison, receiver/prototype mutation and toSorted are not represented by this helper. No new default/fallback sorting path is introduced. Ordinary consistent comparator call sites retain Rust stable sort.

## Proof ledger

`/tmp/pi-rust-repin-lossless-interactive/test-v8-array-sort-red.log`: missing helper, following real picker/search RED where Rust stable sorting with NaN→Equal disagreed at length7. `test-v8-array-sort-green-1.log`:1/1,840 combinations of21 lengths(0–2048),8 run patterns and5 comparators (Date/NaN, asymmetric Boolean, cyclic, stateful alternating, non-finite/fractional result table). Both entire output identity order and every ordered comparator call/result match actual Node. No assertions inspect method calls or source shape.

## Changes from primary implementation

Rust uses a copied index workarray, safe slice moves and a final in-place permutation so generic T needs no Clone and values remain unmoved during callbacks. Each merge owns a temporary index vector instead of V8's reused FixedArray; no JS/GC allocation interface is modeled. PSF notice, license and this change summary are retained. The algorithmic comparison order, minrun, pending-run decisions, galloping and result coercion are unchanged.
