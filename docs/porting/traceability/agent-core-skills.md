# agent-core skills — common pin

Frozen `packages/agent-core/src/harness/skills.ts:1–375`, 4f9e6f8a4. Rust `harness/skills.rs`.

| Frozen symbol / lines | Branch proof |
|---|---|
| `formatSkillInvocation` 35–38 | eight `skill_formatting` tests; retained utility differential :13065–13068. No XML escaping; nonempty extra instructions; only forward slashes delimit dirname (357–375). Repointed oracle found Windows-drive/backslash bug; removed Rust-specific path handling. |
| `loadSkills` 47–73 | 13 `skill_loading` tests and retained `differential_harness_core` loader fixtures: missing/non-directory roots, symlinks, ordered directories |
| `loadSourcedSkills` 82–104 | same focused/differential: provenance on values/diagnostics; typed callback mapping |
| recursive loader 106–181 | root SKILL.md short-circuit, direct root markdown, sorted children, ignored files/subdirs, dot/node_modules skips |
| ignore rule helpers 183–232 | `.gitignore`, `.ignore`, `.fdignore`; shared native npm-ignore engine; loader tests |
| file loader/validation 234–302 | description required/empty/long; malformed metadata warnings; invalid name diagnostic matrix; keeps overlong descriptions |
| frontmatter/resolve/path helpers 304–375 | CRLF normalization, delimiter behavior, symlinks/canonical paths, lexical path helpers; loader/format differentials |

`Skill.description` and `content` now use `JsString`, like name/filePath; formatting keeps native units. File input is decoded at the Node filesystem boundary. No lossy string shim.

Approved deviations: YAML parser diagnostic wording may differ (`serde_yaml_ng` vs TS `yaml`) while parse success/failure matches (user approval relayed by parent 2026-10-22). YAML double-quoted lone-surrogate escape rejection remains the separate approved gap in AGENTS.md. Existing oracle masks only parse-error message text, unchanged in Q9.

Proof logs `/tmp/pi-rust-repin-delta-agent-core/{loaders-green,utility-loader-green,utils-native-green}.log`: 13 loaders +8 formatting tests and retained loader/utility differentials GREEN. No old-pin proof substituted.

[Native IO promise-rejection propagation](harness-nodejs-env.md#native-io-rejection-carrier) preserves rejected Node failures separately from returned errors/diagnostics; deleted-cwd direct frozen caller matrix is the proof.
