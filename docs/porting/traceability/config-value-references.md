# Coding-agent config-value reference traceability

Pinned source: `packages/coding-agent/src/core/resolve-config-value.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

The private parser is observed only through the three selected public helpers. Nineteen direct requests (`12050..=12068`) compare all three outputs against the real pinned module through one persistent oracle process.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `resolve-config-value.ts:11-12` | Accept only case-sensitive ASCII environment names; unbraced references use the longest valid prefix | Focused matrix plus direct requests `12050..=12051`, `12053`, `12057..=12063`, `12066` |
| `resolve-config-value.ts:14-16` | Keep template/reference representation private | Private Rust enums; no public AST |
| `resolve-config-value.ts:18-26` | Ignore empty literals and merge adjacent literal parts | Private representation preserved; focused matrix and direct requests `12052..=12056`, `12060..=12063`, `12066..=12068` prove selected public effects |
| `resolve-config-value.ts:28-40` | Scan left-to-right for dollars and preserve intervening/tail literals | Focused matrix and direct requests `12052..=12063`, `12066..=12068` |
| `resolve-config-value.ts:42-46` | Decode `$$` and `$!` as literal characters | Focused matrix and direct requests `12055..=12056` |
| `resolve-config-value.ts:48-64` | Parse through the first `}`; accept valid full names, preserve invalid segments, and rescan after unterminated braces | Focused matrix and direct requests `12051`, `12053..=12054`, `12060..=12062`, `12066` |
| `resolve-config-value.ts:66-75` | Parse the longest unbraced prefix or preserve an unrecognized dollar | Focused matrix and direct requests `12050`, `12053..=12059`, `12061`, `12063`, `12066`, `12068` |
| `resolve-config-value.ts:80-85` | Give exact leading `!` command precedence | Focused matrix and direct requests `12064..=12065` |
| `resolve-config-value.ts:92-99` | Deduplicate environment names while preserving first occurrence | Focused proof and direct request `12054` |
| `resolve-config-value.ts:115-119` | Return a name only for exactly one environment part | Focused proof and all direct response `envVarName` fields |
| `resolve-config-value.ts:121-124` | Return template names or an empty list for commands | Focused proof and all direct response `envVarNames` fields |
| `resolve-config-value.ts:130-132` | Report command classification | Focused proof and all direct response `isCommand` fields |
| `auth-storage.ts`, `provider-composer.ts` | Consume command classification and environment-name discovery | Consumer mapping only; Rust integration remains out of scope |

Rust parses valid UTF-8 byte offsets while recognizing only ASCII names. This matches every Rust-representable source string. JavaScript lone UTF-16 surrogates and indexing behavior inside such invalid strings remain an explicit typed-string exclusion.
