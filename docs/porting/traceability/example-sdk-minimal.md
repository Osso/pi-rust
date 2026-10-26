# SDK minimal example

Source: pinned `packages/coding-agent/examples/sdk/01-minimal.ts` at
`4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` ([source lock](../source-lock.json)).
[Contract](../../specs/example-sdk-minimal.md).

| Source statement / branch | Rust composition | Proof |
| --- | --- | --- |
| `createAgentSession()` with no arguments | `main` passes `CreateAgentSessionOptions::default()` to `run` | Whole pinned program factory oracle asserts zero arguments; native main compiled, not executed against user defaults |
| Subscribe before prompt | `run` installs `session.subscribe` before awaiting `prompt` | Oracle exact event order; native output begins with Faux text |
| `message_update` + `text_delta` only | `AgentSessionEvent::Message` wrapping `AgentEvent::MessageUpdate`, matching `AssistantMessageEvent::TextDelta` | Oracle ignores message start/end and thinking delta; native text and tool-call stream |
| Literal prompt | `run` uses source string and default `PromptOptions` | Oracle records sole prompt argument; native provider sees exact text block |
| Print all `session.state.messages` in order | `session.messages()` serialized as JSON lines | Every native output message equals reopened persisted SDK state; source log arguments normalized to JSON match exact native stdout |
| Final `console.log()` | Final newline after each message newline | Native suffix is two newlines; source oracle records empty log call |
| `finally { session.dispose(); }` success | Dispose after output | Session-scoped resource cleanup callback observes exact fixture session ID |
| Prompt throws | Return SDK error, skip message output, dispose | Native missing-auth fixture checks exact existing guidance, source `Error` name/message, and resource release; pinned source oracle throws after text deltas and still disposes |
| Unhandled failure diagnostic/status | `main` maps errors to `ExampleError`; Rust `Termination` prints `Error: <exact message>` and fails | Isolated test subprocess checks exact stderr (single `Error:` prefix, literal quotes/newlines), exit 1 on error and exit 0 on success; no default main execution or JavaScript frames |

## Proof boundaries

`oracle/sdk-minimal-oracle.ts` reads the actual pinned file, removes only its
single SDK import, and executes the complete remaining body verbatim in a VM
with a controlled factory. Factory arguments, subscription/prompt/write/log/
dispose ordering, message values, prompt failure, and stdout formatted with
Node's `util.format` are observed. This proves the pinned program's contract,
not a live/default SDK session.

`tests/example_sdk_minimal.rs` compiles the exact production example with
`#[path]`. Native fixtures use real SDK composition, in-memory credentials and
settings, Faux native provider, temporary home-root/cwd/agent/session/control
paths, static resources populated by an explicit empty `ExtensionRegistry`, and
disabled inbound coordination. No default resource discovery, bundled host
hooks, provider network, login, or user database is exercised. Failure uses an
unregistered provider name: Faux itself intentionally needs no credentials.

## Approved deviation / integration

Alessio approved on 2026-10-20: question, “May runnable Rust SDK examples
print message objects as JSON instead Node inspect syntax and omit JS stack
frames while preserving message values, error names/messages, stream order/disposal?”
Answer: **“JSON + omit JS frames (Recommended)”**.

Literal Node inspect stdout differs from native JSON lines. Earlier literal-output
RED remains in `/tmp/pi-rust-example-sdk-minimal-literal-red-2.log`; current tests
retain the complete source stdout in their logs. The authorized comparison
serializes every source log message argument without dropping any fields, then
compares exact stream prefix/text, message ordering, line breaks, and final blank
line. This is not literal source stdout byte parity; no other normalization is
applied. The source oracle separately asserts default factory arguments, exact
prompt arguments, observer filtering/order, and disposal on both paths.

`ExampleError` renders the exact SDK message without Debug quoting; Rust `Result`
termination supplies the single generic source `Error:` prefix and failure status.
The fixture checks exact native stderr and status in a test subprocess without
executing default main. The SDK
already exposes creation errors as strings and prompt errors with a message;
this example does not change core error types or claim arbitrary structured
provider-error preservation beyond that existing boundary.

Coverage is **done** (coverage.md). Verified 2026-10-26 at `11aa7a810`: `cargo test -p pi-coding-agent --test example_sdk_minimal` passes 4/4 against the pinned-source oracle.
No source bugs identified; no new dependencies or invalid-input exclusions.
Other twelve SDK example modules remain untouched.
