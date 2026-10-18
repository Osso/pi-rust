# Coding-agent config-value reference traceability

Pinned source: `packages/coding-agent/src/core/resolve-config-value.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

The private parser is observed through selected public helpers. On Unix, thirty-four direct requests (`12050..=12083`) compare the pinned module with Rust: nineteen reference/classification requests, fourteen status requests using controlled per-request process environments, and one separately environment-seeded non-Unicode-byte request. Focused status tests independently use self-spawned Rust test processes with cleared environments.

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
| `resolve-config-value.ts:88-90` | Prefer truthy explicit values, then truthy process values; empty strings fall through or become missing | Focused child-process proof plus direct requests `12071..=12080`, `12082..=12083` covering missing, empty, ASCII, Unicode, non-Unicode bytes, explicit values, fallback, and case behavior |
| `resolve-config-value.ts:126-128` | Filter unique names in first-seen order to those without a configured value | Focused proof plus direct requests `12069..=12083`, including mixed and duplicate missing-name order |
| `resolve-config-value.ts:130-132` | Report command classification | Focused proof and all direct response `isCommand` fields |
| `resolve-config-value.ts:134-135` | Report configured when the missing-name list is empty | Focused proof plus every direct status response `12069..=12083` |
## Value resolution, command execution, and headers

Rust: `resolve_config_value`, `resolve_config_value_uncached`, `resolve_config_value_or_throw`,
`resolve_headers`, `resolve_headers_or_throw`, `clear_config_value_cache`.
Focused tests: `crates/pi-coding-agent/tests/resolve_config_value.rs` (10 cases, ports of
`test/resolve-config-value.test.ts`). Differential: `differential_config_value_references/resolve.rs`
adds 15 resolve requests (including three process-environment child probes), 4 throw requests, and 5
header requests against the pinned module.

| TypeScript source | Observable contract | Rust proof |
|---|---|---|
| `resolve-config-value.ts:140-155` | Resolve literals, templates, and escapes; commands only when the value starts with `!` | `resolve_config_value`; focused `resolves_literals_templates_and_escapes`, differential resolve cases |
| `resolve-config-value.ts:157-215` | `execSync` semantics: `/bin/sh -c`, 10 s timeout, stdout trimmed, empty/failed → `undefined`; Windows tries the configured shell first (stdin transport supported) | `execute_with_default_shell`, `execute_with_configured_shell`, `wait_with_timeout`; focused command tests + differential |
| `resolve-config-value.ts:217-232` | Command results cached per process for both success and failure until cleared | `command_result_cache`, `clear_config_value_cache`; `caches_successful_and_failed_commands_until_cleared` |
| `resolve-config-value.ts:235-243` | Uncached resolution re-executes commands | `resolve_config_value_uncached`; `uncached_resolution_executes_a_command_on_every_call` + differential |
| `resolve-config-value.ts:245-268` | Throw messages name the command or the missing environment variables | `resolve_config_value_or_throw`; `throws_name_the_failing_source` + differential throw cases |
| `resolve-config-value.ts:270-287` | Header resolution drops empty results and names the header on failure | `resolve_headers`, `resolve_headers_or_throw`; header focused tests + differential |

| `auth-storage.ts`, `provider-composer.ts` | Consume command classification, environment-name discovery, and configured status | Consumer mapping only; Rust integration remains out of scope |

Rust parses valid UTF-8 byte offsets while recognizing only ASCII names. This matches every Rust-representable source string. Explicit environments are typed own-key `HashMap<String,String>` values; JavaScript inherited/non-string properties remain within the malformed runtime-object exclusion. Process status uses `var_os` because only nonempty presence is observable; direct Node probing confirms a non-Unicode byte value remains nonempty and configured. JavaScript lone UTF-16 surrogates and indexing behavior inside such invalid strings remain an explicit typed-string exclusion.
