# Codex restriction recovery

Frozen source: `core/provider-restriction-recovery.ts` at the [common pin](../porting/source-lock.json).

- Advisory requests require nonblank original request and refusal text, preserve evidence without truncation, and reject JSON contexts beyond 8000 UTF-16 units.
- Forward the caller's signal to the shared Supervisor requester, timeout 45000ms, one attempt. Cancellation before or after the requester blocks recovery.
- Accept only strict blocked/rescope objects. Reject normalized original-task repetitions and non-substring authorization quotes. Supervisor advice grants no execution permission.
- Format the literal tool-free main-model validation prompt, including text-only failure evidence and the full proposal.
- Validation requires normal stop, no nonempty error or tool content, strict four-field JSON, exact task equality and a nonblank exact authorization quote for allowed decisions. Denials retain their reason.
- Typed assistant content excludes malformed JavaScript runtime values; external advisory/validation JSON remains dynamically checked.

Quota fallback order and the literal case-insensitive quota matcher follow `core/codex-quota-fallback.ts`. Only error-stop messages with truthy matching error text qualify. These helpers do not switch models or execute advice.
