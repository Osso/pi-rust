# coding-agent frontmatter traceability

Pinned source: `packages/coding-agent/src/utils/frontmatter.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `26007` executes the real pinned module across 24 valid, invalid, CRLF, block-scalar, nested, and unterminated cases. Focused tests: `crates/pi-coding-agent/tests/frontmatter.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `frontmatter.ts:3-6` | Parsed value exposes `frontmatter` and `body` | `utils::frontmatter::ParsedFrontmatter`; focused parse cases |
| `frontmatter.ts:8` | Normalize CRLF and lone CR to LF | `parse_frontmatter`; focused `normalizes_newlines_and_handles_crlf`, differential CRLF case |
| `frontmatter.ts:10-23` | Require leading `---`; find the first `\n---`; YAML is `slice(4, endIndex)`; body is `slice(endIndex + 4).trim()` | `extract_frontmatter`; focused missing/unterminated/empty cases |
| `frontmatter.ts:24-26` | No YAML string returns `{}` and the normalized body | Focused `returns_original_content_when_frontmatter_is_missing_or_unterminated` |
| `frontmatter.ts:27-29` | Parse YAML; `null` becomes `{}` | `serde_yaml_ng` parse plus `yaml_to_json`; focused comment-only/empty cases |
| `frontmatter.ts:35` | Invalid YAML throws | Rust returns `FrontmatterError`; focused `rejects_invalid_yaml`, differential error cases |
| `frontmatter.ts:39` | `stripFrontmatter` returns the body | `strip_frontmatter`; focused strip cases |

## Discrepancies

- The source uses the JavaScript `yaml` package. The Rust port uses `serde_yaml_ng`; values match across the differential corpus. YAML tags are rejected explicitly instead of being resolved.
- Extraction removes the final line break before the closing `---`; the port restores it for the parser so literal/folded block-scalar chomping matches the source. Differential block-scalar cases cover `|`, `>`, `|-`, blank lines, and document end.
- Error message text differs; differential tests compare success values exactly and error occurrence only.

## Proof inventory

- Focused cases: 9.
- Differential request: `26007`, 24 cases.
