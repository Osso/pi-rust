# Login dialog

Frozen `4f9e6f8a4`, login-dialog.ts; [traceability](../porting/traceability/login-dialog.md).

- Show exact title, focusable input, auth/device hyperlinks, information, progress and waiting hints. Clear or append content at the frozen boundaries. Empty instructions/placeholder strings add no rows.
- Submit resolves only the current prompt with exact UTF-16 input and replaces all displayed references to that input with submitted text. Cancellation aborts the shared signal, rejects the current prompt with Login cancelled, and calls completion on every cancel.
- Showing another prompt replaces its resolver without resolving/rejecting the older promise. A dropped native channel must not invent cancellation; a superseded prompt stays pending.
- Auth URL presentation launches the platform browser with the target as one literal argv value. Device-code presentation does not launch a browser. Encoding occurs only at that OS boundary.
