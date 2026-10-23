# Supervisor startup

Port `supervisor/ensure-running.ts` from the [common pin](../porting/source-lock.json).

- Probe resident identity without attaching. Reuse matching versions. Missing identity is an explicit error.
- Share in-flight startup by resolved control-db path; serialize across processes using the shared proper-lockfile protocol (stale5000ms, refresh1000ms,100 randomized retries with factor1.1 and25–100ms bounds). Re-probe under lock.
- Refuse replacement of externally managed incompatible versions. Terminate only verified Pi runtime processes other than self. Wait for old instance disappearance before launching.
- Preserve compiled/script invocation, runtime flags, home cwd and environment; overwrite `PI_SUPERVISOR_AUTOSTARTED` with1.
- Launch detached and wait for compatible readiness, default timeout10000ms and poll50ms. Exit/readiness failures and lock-release failures remain explicit.

Native launch uses the established `setsid(1)` mechanism and preflights target existence/execute permission. A target disappearing or failing exec after preflight cannot currently be acknowledged exactly; this is **pending user decision**, not accepted parity or an approved deviation. No exec-acknowledgement protocol is implemented.
