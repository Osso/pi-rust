# Read tool traceability

Pinned source: `packages/agent/src/harness/tools/read.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-agent-core/src/harness/tools/read.rs`. Behavioral proof: `tests/harness_tools.rs` read tests. Differential: 9 read fixtures in `tests/differential_harness_tools/`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `read.ts:15-19` | TypeBox schema: `path` required, optional `offset`/`limit`, exact descriptions | `read_schema`; `tool_schemas_match_typebox_json` and differential schema comparison |
| `read.ts:23-25` | `ReadToolDetails { truncation? }` | `details` JSON with `truncation` only when truncated |
| `read.ts:27-36` | `ReadImageProcessor` result union and options (`autoResizeImages` default true) | `ReadImageProcessor`/`ReadImageProcessorResult`/`ImageProcessorOptions`; processor test and differential fixture |
| `read.ts:42-46` | Description text | `DESCRIPTION`; schema test |
| `read.ts:48-52` | Resolve path, read bytes, detect MIME, delegate to processor or encode image | `execute_read`/`read_image`; image tests and differential fixtures |
| `read.ts:53-63` | Processor failure text and hint-appended success text | `read_image`; processor test and differential fixture |
| `read.ts:64-73` | BMP omission without a processor | `read_image`; BMP test and differential fixture |
| `read.ts:74-83` | Text decoding, offset/limit selection, offset-beyond-file error | `execute_read`; offset/limit and offset-error tests and differential fixtures |
| `read.ts:84-99` | Head truncation, first-line-over-limit notice, continuation notices | `execute_read`; 2500-line and exact-2000-line tests plus differential fixtures |
| `read.ts:100-117` | User-limit remainder notice | `execute_read`; offset/limit test |
| `read.ts:119-125` | Return text content and optional truncation details | `AgentToolResult` construction; all read tests |

## Rust boundary and discrepancies

- `offset`/`limit` are `Option<usize>`; JavaScript fractional, negative, NaN, and infinite values are excluded by the typed API.
- `path` is used verbatim in notices, matching the source.
- The image processor is a Rust trait returning a `BoxFuture`; the source async function maps directly.
