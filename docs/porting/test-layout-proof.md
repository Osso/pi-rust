# Integration-test target consolidation

## Layout

Five crates disable automatic integration-test discovery. Their explicit `suite_*.rs`
targets include the original files as named modules. Add new process-local fixtures
to the matching suite; add an explicit standalone target when process isolation is
required. The split coding-agent crates already disable test discovery and reuse
symlinked fixture directories; they need no additional targets.

| Crate | Original file targets | Consolidated targets | Suite / standalone |
| --- | ---: | ---: | ---: |
| pi-coding-agent | 732 | 250 | 12 / 238 |
| pi-ai | 147 | 18 | 4 / 14 |
| pi-tui | 89 | 25 | 3 / 22 |
| pi-agent-core | 75 | 10 | 4 / 6 |
| pi-js-compat | 53 | 6 | 2 / 4 |

The 812 process-local files share 25 suite binaries. The other 284 file targets
retain their original process boundary. Existing `harness = false` targets remain
separate, including `terminal_stream` and `bash-runner-lifecycle`.

The isolation audit follows local `mod`, `#[path]`, and literal `include!` helpers.
Files using self-executable probes, process-environment or cwd mutation, global
hooks, provider-registry or terminal-global setters, or crate-root references retain
standalone targets. `ModelRegistry::register_provider` also writes the process-wide
compat/OAuth registries; its fixtures and scripted-provider helpers remain isolated.
Probe names therefore
need no rewrite. Helpers retain their original fixture-local types, statics and
paths except `mode_harness`, which is declared once in `suite_modes`: its atomic
counter must be shared because the package-directory prefix contains the PID.
Separate helper copies in one process otherwise produce identical paths.
Public wrapper modules preserve the original visibility of public fixture helpers.
The settings/theme recursion limits move from their old crate roots to suite roots.
No assertions, product code, profiles, dependencies, runner settings, fixture paths,
or binary environment names change.

## Proof ledger

Baseline source: `09f10e9c073adba93dbf60eda3ffca1e4578cc23`.
Consolidated source after isolation corrections: `6cd6501a4`.
The timing sample below covers `d8a7710ed`; five more targets were restored to
standalone execution afterward, without changing their test bodies. Main merged
the UI split `91a269c01` as `e262279ca` after the full-suite comparison; final
check/list gates cover that integration, not another full suite.
Raw lists, parsed names, the 6,364-entry name bijection, command logs, isolation
classification, and measurement stamps are saved in `/tmp/test-consolidate-proof/`.

- Before/after `cargo test -j8 --workspace -- --list`: 6,364 entries each,
  including the unchanged documentation test. Every original `(crate, target,
  name)` maps exactly once to either itself or `(crate, suite, file::name)`.
- First slice: 26 targets become one; all 98 tests pass in 10.24 seconds.
- `cargo check -j4 --workspace --all-targets --keep-going --message-format=json`:
  success, zero errors and warnings after moving root-only attributes.
- `cargo fmt --all --check`: PASS.
- `python3 scripts/source-inventory.py`: PASS, 686 rows / 186,679 source lines.
- Rust readability: added Rust files contain only explicit module declarations
  and root attributes; original test bodies are unchanged.
- Full before workspace suite: 50 failed targets / 76 failed test outcomes, plus
  two stalled tests in the unchanged interactive library; see
  [baseline assignments](test-layout-baseline-failures.md).
- Full after workspace suite plus parent-approved targeted final corrections:
  all 6,364 outcomes are accounted for, with no unknown or missing cases. There
  are 75 failed outcomes and the same two interactive timeouts. Nine unchanged
  cases vary from baseline and are explicitly accepted as **FLAKY** by main in
  [baseline assignments](test-layout-baseline-failures.md). The other 6,355
  outcomes match. Strict pass/fail-set equality was not obtained.
- Parent-approved corrections replace the full run's affected extension/tools,
  mode and AI-core suite results with targeted reruns. The corrected 327-case
  extension/tools scope exactly matches baseline (321 pass, six existing
  failures); the 237-case mode scope also matches (234 pass, three existing
  failures). The fixed-port Anthropic OAuth fixture retains its own sandbox;
  one unchanged OAuth case remains FLAKY. No tests were serialized.

## Build measurements

Both measurements use `cargo test -j8 --workspace --no-run`, the same manifest
profiles and `incremental = false`. Cold means an absent worktree target directory;
it does not flush the OS page cache. Touch measurements update the mtime of
`crates/pi-coding-agent-tools/src/lib.rs`, with no content change. Sizes use `du -sh`.

| Run | Before seconds | After seconds | Before / after uptime seconds |
| --- | ---: | ---: | --- |
| Cold | 574.4 | 365.7 | 1330439.73–1331014.10 / 1334488.26–1334853.99 |
| Tools file touched | 852.1 | 176.2 | 1331346.48–1332198.56 / 1334854.94–1335031.14 |
| Cold target size | 68 GiB | 33 GiB | Same completed cold builds |

Observed reductions: cold time 36%, touched-file time 79%, disk use 51%.
These are single samples, not controlled benchmarks. Concurrent host builds were
active; the before-touch run overlapped the parent workspace rebuild. The full
baseline suite runs read-only in the parent worktree; its incremental-only merge
`bbc0ed0efffbf177c09ce2cda9724660b234f587` does not change test source but does
change its build configuration. Build timing comparisons above use only this
worktree, never the parent suite's rebuild time. The final cold measurement used
an empty `/var/tmp/osso/pi-test-consolidate-cold` directory on the same filesystem,
concurrently with both full suites; that temporary target was removed after the
measurement and final check. The first after suite was stopped before grouped
coding-agent suites ran because the second audit found additional global setters;
its partial output is retained, not counted as final acceptance. A later run
exposed `process_cwd::chdir` in `example_built_in_tool_renderer`, `bash_stdin`,
and `differential_bash_restored`, plus `set_child_subreaper` in
`example_rpc_extension_ui`. These files now retain standalone targets. The cwd
mutation produced a deleted-directory recovery frame in another RPC test; that
unexpected frame and the RPC hang disappear after isolation. No sleeps, test
threads, assertions, or product behavior were changed. The superseded tools
sandbox was terminated so the full run could continue; only the corrected
327-case proof contributes those cases to the final ledger. The print/RPC
helper-counter collision was reproduced as `AlreadyExists` at
`support/mode_harness.rs:359`; one shared module fixes it without changing the
helper or any assertion. Final workspace integration-test targets fall from
1,112 to 325; existing unit and documentation tests remain.
