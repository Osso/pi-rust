# Native Error constructor classes

Contract: [spec](../../specs/native-error-classes.md). One constructor marker in `pi_js_compat::node_error::NativeNodeError`; Error/DOMException outer branding remains in [AbortReason](shared-abort.md).

Observed dependency: pinned Mistral `lib/http.ts::isConnectionError` uses actual `instanceof TypeError`, not name equality. Whole-provider paired RED (`mistral-typeerror-class-red.log`) shows plain Error named TypeError gives `Unexpected HTTP client error: TypeError: fetch failed`, while a real TypeError gives `Unable to make request: TypeError: fetch failed`; native previously returned identical `fetch failed` for both. No source bug or semantic deviation is inferred.

| Carrier behavior | Proof |
|---|---|
| `NativeErrorClass`, `from_constructor` canonical name and subclass membership | Direct Node constructor matrix, eight classes × three name states |
| `new` remains generic Error even when named TypeError | Explicit class assertions; paired provider source evidence |
| `is_instance_of(Error)` accepts all built-in subclasses; specific checks read marker | All eight instanceof results per fixture |
| name mutation, native UTF-16 message/name, exact error-record output without class metadata | Same24 fixtures compare record and full instanceof vector |

`native-error-classes-green.log`: targeted shared-crate differential GREEN. This carrier commit does not establish Mistral wrapping or factory migration; those follow separately. V8 stacks omitted by existing approval.
