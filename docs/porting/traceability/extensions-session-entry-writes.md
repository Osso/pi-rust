# extensions/session-entry-writes.ts

Frozen `4f9e6f8a4` (24 lines). Native equivalent: owned arguments in `SessionManager`, used by `SessionSetupCallback` (`extensions/types.rs`). No proxy/wrapper/API carrier is necessary in Rust.

| Frozen method | Ownership boundary / proof |
|---|---|
| appendMessage | typed AgentMessage is owned/deep-cloned before transfer |
| appendCustomEntry | native JsValue is owned; clone detaches nested caller data |
| appendCustomMessageEntry | content/details are owned values |
| appendCompaction | copied scalar strings and owned details/providerNative options |
| branchWithSummary | copied scalar strings and owned details |
| non-write property/method binding | setup retains the full mutable SessionManager, with no JS receiver-binding requirement |

`setup_writes_detach_all_five_caller_payloads_like_frozen` compares all five retained payloads with the actual frozen `createSessionSetupWriteView`; mutating caller objects after writes changes neither implementation's stored values. Also checks non-write cwd access. GREEN `/tmp/ext-api-setup-writes3.log` at 1dcf30994 plus new tests. No production code change was needed. This proves owned-native semantics, not JS identity.

Gap: adversarial non-structured-cloneable JS runtime values are excluded by the realistic-input parity bar; the native data carriers do not contain callbacks, symbols or JS prototypes.
