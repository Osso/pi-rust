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

## Request boundary producer migration (2026-10-25)

`pi-ai/src/request.rs` header ByteString and URL failures now construct genuine native TypeErrors instead of renaming a plain Error. Actual frozen version-release HTTP input exposed the header constructor mismatch; `lossless_auth_interaction::request_boundary_errors_retain_typeerror_constructor` reproduces both producers. The same-pattern search found and migrated the catalog failure fixture to the source's genuine TypeError with its cause. Focused GREEN pending.

## Q15 nested causes

NativeErrorDetails now owns one IndexMap<JsString, AbortReason>; with_property wraps JSON values, with_cause/cause retain branded nested values, and value() projects only when requested. Existing boxed NativeNodeError size, property order and string conversion remain unchanged. `native_error_causes_keep_branding_values_order_and_string_conversion` compares actual Node raw-surrogate/null/lookalike/Error-with-cause/DOMException values, branding, string conversion and exact key order; also checks clone equality and replacement of the cause slot. API RED and catalog constructor-eligibility behavioral RED were recorded first. At `3e532c89`, both shared differential tests pass (24 constructor/name and5 cause fixtures), and cargo check --workspace --all-targets passes with zero warnings. This shared carrier is not catalog retry/cause acceptance.

## Q5 Node-internal code-aware strings (2026-10-23)

Ordinary Error/TypeError objects with assigned code properties retain Error.prototype.toString. Regular Node-internal error constructors capture the original constructor code in their string override; later code-property mutation does not change the displayed code. SystemError's existing override instead reads its current code property. NativeNodeError stores the captured regular-NodeError code separately and exposes from_node_constructor; URL/path-validation and abort-timeout internal errors use it. No code-property heuristic or fabricated class is used.

Parent runtime RED at 4163333d (`/tmp/pi-rust-repin-main-differential_config_error_sources.log`) observed the missing `[ERR_INVALID_FILE_URL_HOST]` prefix. `differential_node_error_strings` compares 120 actual Node internal-constructor/ordinary-error cases, varying code, name and message (including empty strings). The getter/consumer Result cascade remains separate.

Node 26 URL-constructor exception: ERR_INVALID_URL is a plain TypeError with code/input properties, not a captured-code NodeError override. The frozen invalid-URL fixture exposed this distinction after the initial prefix fix; native URL parsing now selects that constructor identity explicitly. Host/path file-URL validation remains captured-code NodeError.
