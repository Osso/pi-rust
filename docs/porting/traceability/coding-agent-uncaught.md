# utils/uncaught.ts — native uncaught payloads

Rust `utils/uncaught.rs`: `UncaughtError.message: JsString`; `throw_uncaught(label,impl Into<JsString>)` retains the payload; `format_uncaught(...)->JsString` formats without encoding. Existing process hooks still exit1 and invoke the crash handler; only actual `eprintln` encodes Node UTF8. Label tags remain static scalar literals.

`raw_uncaught` exercises the actual panic carrier/formatter plus three child processes running the real installed panic hook: high/low/pair stdout-independent stderr bytes match Node UTF8, exit1. Native API compile RED preceded implementation. `raw_theme_error` uses the same actual uncaught module for the Error label. This does not replace source process/crash-handler lifecycle proofs.
