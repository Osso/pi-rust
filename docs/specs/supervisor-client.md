# Supervisor client

Frozen module: `supervisor/client.ts` at the [common pin](../porting/source-lock.json).

- Ensure the resident is running before posting each attempt. Startup failures become error responses; cancellation is checked before startup and again afterward.
- Post a durable request with the source Date/ISO deadline and preserve request fields. Poll completed or cancelled requests; default interval300ms.
- Forward the shared reason-bearing signal. The client deliberately reports the module's fixed cancellation reason, not the caller's AbortSignal reason. Abort synchronously cancels pending/claimed durable requests, clears claim ownership and wakes the service; polling and retry sleeps stop promptly. Remove the listener when the attempt exits.
- Preserve source JS-number attempt/deadline/timer semantics, exponential retry jitter and final timeout suffix. Default attempts1, base delay1000ms, jitter0.2. Timeout does not cancel the durable request.
- Invalid Date deadlines raise the native RangeError rather than coercing or saturating them. Database failures remain explicit native errors.
- Optional startup dependency injection is the frozen test/embedder seam; the production default uses [startup coordination](supervisor-ensure-running.md), not a no-op.
