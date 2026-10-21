# Supervisor ensure-running frozen traceability

Reference: `4f9e6f8a4`, `supervisor/ensure-running.ts`.

| Export/branch | Rust | Proof |
|---|---|---|
| start-lock path, launch invocation | same-named snake-case exports | `q16_startup::launch_invocation_matches_compiled_and_script_runtimes` |
| matching resident reuse; coalescing; launch/readiness; external mismatch; lock release | `ensure_supervisor_running` | `q16_startup::startup_launches_once_and_rejects_external_mismatch` |
| managed replacement/exit timeout, launch error, readiness timeout, cross-process contention | `ensure_under_lock`, polling helpers | pending focused differential |
| default probe | `probe_supervisor_identity`, transport probe | cross-language resident probe; startup default unavailable/error branches pending |
| terminate verification; process launch | `terminate_supervisor`, `launch_supervisor` | compiled only; no live services allowed |

No row promoted to done yet. External `setsid` approved by parent 2026-10-21; no fallback.

Q16 six-case frozen startup trace matrix now covers ready reuse, absent launch, external mismatch, managed replacement, zero-timeout, and launch error using process fakes. Versions are explicit fixture parameters for that algorithm matrix; the separate unignored exact-version assertion fails (Rust 0.84.1 vs frozen 0.80.3). No raw-output parity claim.
