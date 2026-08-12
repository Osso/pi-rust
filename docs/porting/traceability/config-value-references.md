# Coding-agent config-value reference traceability

Pinned source: `packages/coding-agent/src/core/resolve-config-value.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

The current slice maps the private parser only through the three selected public helpers. Focused tests cover every representable selected branch; direct TypeScript differential proof remains the current-cycle gap.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `resolve-config-value.ts:11-12` | Accept only case-sensitive ASCII environment names; unbraced references use the longest valid prefix | Focused exact, prefix, digit, underscore, case, Unicode, and invalid-name fixtures |
| `resolve-config-value.ts:14-16` | Keep template/reference representation private | Private Rust enums; no public AST |
| `resolve-config-value.ts:18-26` | Ignore empty literals and merge adjacent literal parts | Private representation preserved; escape, malformed-brace, surrounding-literal, and repeated-dollar fixtures prove selected public effects |
| `resolve-config-value.ts:28-40` | Scan left-to-right for dollars and preserve intervening/tail literals | Focused literal, adjacent-reference, Unicode, NUL, prefix, and suffix fixtures |
| `resolve-config-value.ts:42-46` | Decode `$$` and `$!` as literal characters | Focused escape and repeated-dollar fixtures |
| `resolve-config-value.ts:48-64` | Parse through the first `}`; accept valid full names, preserve invalid segments, and rescan after unterminated braces | Focused valid/invalid/first-close/unterminated/later-reference fixtures |
| `resolve-config-value.ts:66-75` | Parse the longest unbraced prefix or preserve an unrecognized dollar | Focused `$A-B`, `$A.B`, `$A1x`, Unicode, digit-start, lone-dollar, and punctuation fixtures |
| `resolve-config-value.ts:80-85` | Give exact leading `!` command precedence | Focused command, embedded-reference, escaped-bang, and non-leading-bang fixtures |
| `resolve-config-value.ts:92-99` | Deduplicate environment names while preserving first occurrence | Focused repeated bare/braced reference ordering |
| `resolve-config-value.ts:115-119` | Return a name only for exactly one environment part | Focused exact and every surrounding/multiple/literal/command rejection fixture |
| `resolve-config-value.ts:121-124` | Return template names or an empty list for commands | Focused template and command matrices |
| `resolve-config-value.ts:130-132` | Report command classification | Focused exact leading/non-leading/escaped cases |
| `auth-storage.ts`, `provider-composer.ts` | Consume command classification and environment-name discovery | Consumer mapping only; Rust integration remains out of scope |

Rust parses valid UTF-8 byte offsets while recognizing only ASCII names. This matches every Rust-representable source string. JavaScript lone UTF-16 surrogates and indexing behavior inside such invalid strings remain an explicit typed-string exclusion.
