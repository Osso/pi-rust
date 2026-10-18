# coding-agent git traceability

Pinned source: `packages/coding-agent/src/utils/git.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `26008` executes the real pinned module across 55 URL, shorthand, ref, host, percent-encoding, and unsafe-path cases. Focused tests: `crates/pi-coding-agent/tests/git.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `git.ts:6-19` | `GitSource` exposes `type`, `repo`, `host`, `path`, optional `ref`, `pinned` | `utils::git::GitSource`/`GitSourceType` with camel-case wire names and omitted `ref` when absent; focused serialization test |
| `git.ts:21-74` | `splitRef` handles scp-like, protocol, and host/path forms; empty path or ref keeps the original URL | `split_ref`; focused ref/hash cases, differential `26008` |
| `git.ts:76-102` | Unsafe install parts reject malformed percent escapes, NUL, backslash, absolute paths, and `..` segments | `has_unsafe_git_install_part` and `decode_uri_component`; focused unsafe-path test |
| `git.ts:104-124` | Normalize `.git`, require host and at least two path segments, reject leading slash | `build_git_source`; focused incomplete-repository test |
| `git.ts:126-163` | Generic parser accepts scp-like and explicit protocols, requires a dotted host or `localhost` for shorthand | `parse_generic_git_url`; focused cases |
| `git.ts:172-180` | Without `git:` prefix only explicit `http(s)`, `ssh`, `git` URLs are accepted | `has_explicit_protocol`; focused rejection test |
| `git.ts:181-225` | Hosted parsing tries ref-qualified candidates, then https-prefixed candidates, then generic parsing | `hosted_git_info` + `parse_git_url`; differential `26008` |

Hosted parsing ports the pinned `hosted-git-info@9.0.3` subset: GitHub, Bitbucket, GitLab, Gist, SourceHut, shortcut protocols, `www.` stripping, extraction rules, and `decodeURIComponent` failure handling.

## Discrepancies

- Source bug preserved: `git://github.com/user/repo` produces `repo = "https:////github.com/user/repo"` because the hosted candidate misses and the generic parser prepends `https://` to an already-prefixed URL. Focused `preserves_source_bug_for_git_protocol_repo_url` and differential case.
- Source quirk preserved: `https://github.com/user/repo@main#x` yields `ref = "x#main"` and `repo` ending in `#x`. Focused `preserves_hash_and_ref_interaction_from_source`.
- Gist single-segment URLs produce `path = "null/abc123"` because the source interpolates a `null` user; preserved and covered by differential cases.
- `url` crate parsing replaces the JavaScript WHATWG parser; differential cases cover protocols, credentials, ports, case, and percent-encoding.

## Proof inventory

- Focused cases: 8.
- Differential request: `26008`, 55 cases.
