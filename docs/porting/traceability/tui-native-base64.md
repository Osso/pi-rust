# Native Buffer base64 decode for TUI image consumers

Parent authorized this shared primitive and narrow caller adaptation. `buffer_from_base64`
now accepts `Into<JsString>` and has one native-unit implementation; no scalar fallback.
Real Node Buffer.from reads each UTF-16 unit's low byte for base64. Verified before
implementation: QQ😀QQ decodes to hex41, whereas old Rust's UTF-8 traversal yielded
410410. Targeted RED reproduced it; raw high/low/pair/wide-low-byte/padding/alphabet
cases now match real Node GREEN. At that checkpoint atob and buffer_decoded_len were unchanged; the later native-atob checkpoint below supersedes atob.

`raw_base64` (11 cases) and existing `differential_base64` ran with mandatory runner,
jobs4. Iterator callers in differential_base64 and differential_truncate explicitly
unwrap &&str; remaining valid &str/&String callers lift directly into the same native
implementation. SandboxReadOperations::{read_file,read_range} now pass native returned
`data` directly, removing their premature encode_node_utf8 calls. Other coding consumer
behavior was not changed. Whole-workspace/coding acceptance remains parent's gate.

## Complete caller inventory

- `crates/pi-agent-core/tests/differential_harness_tools/mod.rs`
- `crates/pi-agent-core/tests/differential_truncate/base64_decode.rs`
- `crates/pi-agent-core/tests/harness_tools.rs`
- `crates/pi-coding-agent/src/bundled_extensions/bwrap/index.rs`
- `crates/pi-coding-agent/src/bundled_extensions/codex_image_generation/mod.rs`
- `crates/pi-coding-agent/src/bundled_extensions/codex_usage/mod.rs`
- `crates/pi-coding-agent/src/bundled_extensions/openai_remote_compact.rs`
- `crates/pi-coding-agent/src/utils/image_convert.rs`
- `crates/pi-coding-agent/src/utils/tool_result_images.rs`
- `crates/pi-coding-agent/tests/differential_image_pipeline.rs`
- `crates/pi-coding-agent/tests/example_ssh.rs`
- `crates/pi-coding-agent/tests/file_processor.rs`
- `crates/pi-coding-agent/tests/read_tool.rs`
- `crates/pi-coding-agent/tests/support/image_fixtures.rs`
- `crates/pi-coding-agent/tests/tool_result_images.rs`
- `crates/pi-js-compat/tests/base64.rs`
- `crates/pi-js-compat/tests/differential_base64.rs`
- `crates/pi-js-compat/tests/raw_base64.rs`
- `crates/pi-tui/src/terminal_image.rs`
- `crates/pi-tui/tests/differential_terminal_image.rs`

## Native atob checkpoint (2026-10-22)

Parent-authorized `atob(&JsString)` strips HTML whitespace and validates original UTF-16 units without replacement. `Base64Error` distinguishes invalid characters from incomplete encoding and exposes Node DOMException name/message. No alternate scalar API remains. Scalar fixtures promote into the same native entrypoint; Codex token splitting passes original native segments.

At `45107165`, `atob-red.log` reproduced the missing native API/error surface. `atob-green.log` records **7/7 mandatory sandbox tests PASS**: five original base64 tests, pinned TS/Node differential, and one real-Node raw-unit differential over 3,911 inputs (lone high/low units, >255, HTML whitespace, alphabet/padding/invalid-character ordering). The later f033187c whitespace cleanup and expanded fixture proof below supersede this source revision. It does not prove pi-ai/coding-agent integration acceptance.

At `f033187c`, whitespace stripping became the single validation authority and explicit VT/NBSP/BOM/surrogate-pair fixtures were added. `atob-final-green.log` records **7/7 sandbox PASS**, including the real-Node differential over3,915 native-unit inputs. This is the current module proof; no later source change intersects it.
