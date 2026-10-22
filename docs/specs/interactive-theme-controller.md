# Interactive theme controller

Frozen `4f9e6f8a4` contract; [traceability](../porting/traceability/theme-controller.md).

- Resolve startup theme from the setting and environment. Initialize it, then register the terminal-theme listener for the TUI lifetime; no old-only renderer-rebind API.
- Explicit settings disable auto-sync and apply the named theme. Auto settings query the optional color scheme first; a value returns immediately without background query. Unsupported/empty/rejected scheme proceeds to background detection and then frozen environment detection. Queries retain separate timeout budgets.
- Unset settings use background detection and persist only high-confidence results. Theme application invalidates UI before notifying the host; failed load falls back to dark and optionally shows the exact error.
- Auto-sync notifications change only on state transitions. Terminal events are ignored while disabled; active auto settings resolve terminal light/dark, avoid unchanged-theme notifications, and disable sync if the setting ceases being auto.
- Preview invalidates and requests render without notifying the host. In-memory themes disable auto-sync and mark the selection in-memory. Dropping the caller's controller handle does not unregister the retained TUI callback.
