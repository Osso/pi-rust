# Child response

- Capture the child message identities before prompting. Only messages not present in that snapshot can supply this turn's result.
- If the last current assistant message ended with error, fail the child with its nonempty error message or `Child model request ended with error`.
- Otherwise scan current assistant messages backwards, joining text parts with newline; return the first nonempty result, or no result.
- Preserve all UTF-16 units. Equal-valued messages with different identities remain distinct, including after inherited context or branch changes.

[Proof and native mapping](../porting/traceability/child-response.md).
