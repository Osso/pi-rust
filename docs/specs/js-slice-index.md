# JavaScript slice index

`pi-js-compat::number::js_slice_index(index, length)` is the shared relative-index authority for read lines and RTK ls output.

- [x] Apply ToIntegerOrInfinity before testing sign; negative fractions in(-1,0) become-0 and select0, not length.
- [x] NaN selects0; +Infinity selects length; -Infinity selects0.
- [x] Negative integers count from length; clamp the final index into[0,length].

Read's earlier private helper tested sign before truncation. Actual frozen negative-fraction/subnormal limits reproduce the resulting full-file read instead of an empty slice. RTK's negative fractional/NaN cases independently exercise the same conversion through actual executable output. The helper returns an index, not a replacement integer interpretation of the original option.

Proof: `differential_read_numbers`17 controls and `differential_ls_numbers`17 injected +17 RTK controls; `ls-numbers-green-1.log`8/0 with ordinary ls regressions. No dependency added.
