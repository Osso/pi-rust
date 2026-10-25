# coding-agent supervisor project-resolver traceability

Pinned source: `packages/coding-agent/src/supervisor/project-resolver.ts` at `4f9e6f8a4`.

Baseline repair (2026-10-25): config differential serialization uses the lossless serializer for `JsString` object keys, then parses its JSON bytes for comparison. Direct serde_json serialization rejects the native key carrier before any observable comparison. The fixture still compares all config fields, resolution results and URL identities exactly.

Differential proof: request `27011` reads three real config files (populated, `{}`, missing) through both implementations, resolves ten cwd/remote combinations, and extracts identities from nine URLs. Focused tests: `crates/pi-coding-agent/tests/project_resolver.rs`, including real `git remote -v` fixtures.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `DEFAULT_SUPERVISOR_KB_DIR` | `/syncthing/Sync/KB` | `DEFAULT_SUPERVISOR_KB_DIR` |
| `SupervisorProjectMapping` | Optional `remotes`/`roots` string arrays | `SupervisorProjectMapping` (`Option<Vec<String>>`), serialized without absent fields |
| `SupervisorProjectConfig` | Ordered record of project mappings | `SupervisorProjectConfig` (ordered pairs) with JS key ordering applied on read; `honors_javascript_integer_key_ordering` |
| `extractRemoteRepositoryIdentity` | Trims, strips trailing `/`, `.git`, scheme/authority, scp prefix; keeps the last two segments | `extract_remote_repository_identity`; `extracts_repository_identities_and_names_from_remote_urls`, differential `27011` |
| `extractRemoteRepositoryName` | Last identity segment | `extract_remote_repository_name` |
| `resolveSupervisorProjectForCwd` | Reads `<kb>/memory/supervisor/projects.json`, then resolves | `resolve_supervisor_project_for_cwd` (returns `Result`); `resolves_from_the_kb_configuration_and_git_remotes` |
| `readSupervisorProjectConfig` | Missing file → `{}`; non-object → `Supervisor project config must be an object: <path>`; non-object mapping → `Supervisor project mapping must be an object: <id>`; non-string arrays → `Supervisor project mapping values must be string arrays` | `read_supervisor_project_config`; `reads_and_validates_the_project_config_file` |
| `resolveSupervisorProject` | Root match, then remote match (case-insensitive), then first remote's last segment lowercased, then cwd basename | `resolve_supervisor_project`; six focused tests, differential `27011` |
| `findProjectByRoot` | `path === root` or `path` starts with `root + sep` | `is_path_within_root`; `does_not_match_a_sibling_directory_sharing_a_root_prefix` |
| `findProjectByRemote` | Case-insensitive name membership | `find_project_by_remote`; `groups_multiple_repositories_by_configured_remote_names` |
| `readRemoteRepositoryNames` | `git remote -v` second field per line, identity + name, deduped in order; failure → `[]` | `read_remote_repository_names` over `utils::child_process::spawn_process_sync`; `reads_unique_remote_identities_and_names_in_order` |

## Exclusions

- `resolve(cwd)` is a lexical normalization against the process cwd (`utils::paths::lexical_normalize`), not a filesystem canonicalization, matching Node.
- Config keys are ordered explicitly because `serde_json` object iteration order is not guaranteed to match `Object.entries`; canonical array-index keys sort first.

## Discrepancies

- None.

## Proof inventory

- Focused cases: 11.
- Differential request: `27011` (10 resolutions, 9 URL identities, 3 config reads).
