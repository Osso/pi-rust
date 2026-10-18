# Coding-agent file processor traceability

Pinned source: `packages/coding-agent/src/cli/file-processor.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/cli/file_processor.rs`.

Process module (filesystem + image pipeline). Behavioral proof: `tests/file_processor.rs` (6 tests, ports of the `processFileArguments` cases in `test/block-images.test.ts` and `test/image-resize-callers.test.ts`).

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `file-processor.ts:26-29` | `autoResizeImages` defaults to true | `ProcessFileOptions` default; PNG/BMP tests |
| `file-processor.ts:34-41` | Resolve `~`/screenshot variants, then report `File not found: <absolute>` | `reports_missing_files_with_the_source_message`, `resolves_relative_paths_against_the_working_directory` |
| `file-processor.ts:44-47` | Empty files are skipped | `skips_empty_files_and_keeps_argument_order` |
| `file-processor.ts:49-52` | MIME sniffing decides image vs text | PNG/BMP/text tests |
| `file-processor.ts:53-78` | Omitted images append `<file name="...">message</file>`; processed images append hints or an empty element and push an attachment with the processed MIME type | PNG/BMP tests; omitted path is covered by the image-process slice |
| `file-processor.ts:79-88` | Text files append `<file name="...">\n<content>\n</file>\n`; read failures report `Could not read file <path>: <message>` | `processes_text_files_normally` |

## Semantics

- The source prints errors and calls `process.exit(1)`; Rust returns `FileProcessorError` with the same message (the CLI prefixes `Error: ` like the source's `console.error(chalk.red(...))`).
- Text reads use `String::from_utf8_lossy`, matching Node's `readFile(..., "utf-8")` replacement behavior for invalid sequences.
- `process.cwd()` is an explicit `cwd` parameter, matching `resolveReadPath(fileArg, process.cwd())`.

## Gaps

- `process.exit(1)` is the caller's responsibility.
- The omitted-image branch needs an image that cannot be normalized; covered by `utils::image_process` tests.

## Discrepancies

None found.
