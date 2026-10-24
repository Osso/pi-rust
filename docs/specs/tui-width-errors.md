# TUI width errors

Frozen `tui.ts:2193–2220` writes its crash log, calls stop, then throws its exact width Error. A stop failure wins and is preserved unchanged.

Renderer/frame-commit and synchronous/queued render APIs return `Result<_, CallbackError>`. Errors propagate to the outer application render-loop boundary, which uses the shared uncaught authority. The width condition is frozen runtime behavior, not a Rust panic invariant. Flow-region partial commits preserve the same Result carrier.

`raw_frame::width_error_preserves_cleanup_throw_precedence_and_crash_log` compares actual frozen/native errors and crash-log bytes (excluding timestamp), with a stop-success case and raw lone-surrogate stop rejection. The log must exist when stop runs. Both native cases originally panicked instead of returning the original thrown value. No fallback, suppression or catch-unwind transport.
