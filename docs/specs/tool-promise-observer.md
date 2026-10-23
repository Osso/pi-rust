# Built-in tool Promise observer

`core/tools/abortable` shares the frozen read/ls Promise settlement authority.

- [x] Pre-abort rejects with Operation aborted without starting the producer.
- [x] Subscribe before spawning owned work. Abort and producer completion settle exactly once; later settlements are ignored, including after observer drop.
- [x] Abort returns promptly while the awaited operation remains gated. Retain the producer through settlement; do not cancel its subsequent operations or add signal checks absent from frozen.
- [x] Remove the subscription when producer work finishes. No abort-reason substitution, panic-based propagation or new dependency.

`differential_ls_abort`: nine actual-frozen gated scenarios cover exists/root stat/readdir/entry stat, post-abort success/rejection and pre-abort. Exact callbacks prove later stat/readdir still run. Existing `differential_read_abort` proves the moved read authority remains exact. `ls-abort-producer-red.log` fails all eight active-operation cases; `ls-read-abort-green-1.log`2/0.

The old pre-repin cancel-on-abort approval no longer describes read/ls at the common pin. Other users of dropping `race_with_abort_signal` remain separately scoped; this helper does not change them.
