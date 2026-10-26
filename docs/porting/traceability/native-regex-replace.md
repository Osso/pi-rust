# Native regex-replace

Sources: `/home/osso/.worktrees/pi-rust-ext-sources/regex-replace/{extensions/regex-replace.ts,src/pi/json-cli-runner.ts,src/pi/regex-replace-tool.ts}` (user-supplied snapshot).
Rust: `crates/pi-coding-agent-extensions/src/native_user_extensions/regex_replace/`.
Contract: [native regex-replace](../../specs/native-regex-replace.md).

Whole-module audit covers all three supplied files. Existing `ExtensionRegistry::register_hidden` registers `<native:regex-replace-pi-tool>` through the normal registry setup. Read-only `ExtensionApi::resolved_path()` supplies the `import.meta.url` equivalent; no hard-coded repository path, alternate CLI path or new dependency is used.

| Source features | Native implementation | Behavioral proof (`native_regex_replace_*`) |
| --- | --- | --- |
| factory, exact schema/metadata, execute | `mod.rs` | `schema_and_prompt_contract`, path-driven fixture load |
| request-file transport, cwd, cleanup, stderr/status and JSON validation | `cli_runner.rs` | `plans_applies_and_cleans_request_files`, `errors_and_precancelled_signal`, `planning_abort_and_malformed_json_fail_without_apply` |
| operation mutex, defaults, one `@` removal, dry run, sorted unique targets, shared file queues | `operation.rs` | `dry_run_and_explicit_limits`, `plans_applies_and_cleans_request_files`, `file_queue_and_operation_mutex_preserve_order` |
| cancellation before planning, planning cancellation, uncancellable apply | factory / transport | `errors_and_precancelled_signal`, `planning_abort_and_malformed_json_fail_without_apply`, `apply_ignores_late_abort` |
| output, JSON-byte-bounded details, render call/result, coloring, pluralization, partial and invalid details | `presentation.rs` | `bounds_output_and_render_details`, `plans_applies_and_cleans_request_files`, `renderers_keep_call_summary_diff_and_error_text` |

RED: the initial three fake-executable integration tests fail on the missing native factory. GREEN: nine focused tests use a tempdir package entry and executable fake CLI; they perform real process spawning, JSON-file exchange, cleanup and mutation-queue interaction without a network or the real replacement CLI.

Checkpoint proof: nine regex-replace tests and the disable-broadcast regression pass (10/10). `cargo check -j3 --workspace --all-targets`, `cargo fmt --check`, `cargo clippy -j3 -p pi-coding-agent-tools --all-targets -- -D warnings` and additional extensions all-target clippy pass. Source inventory passes unchanged at 686 rows / 186679 lines. Manual changed-Rust readability audit found no remaining algorithmic nesting or complexity issues; schema/registration records remain declarative. Gate logs: `/tmp/native-regex-{check,fmt,clippy,extensions-clippy,test}.log`.

Recorded runtime gap: exceptional temporary-filesystem I/O failures use native error text; normal CLI stderr/status, JSON parse errors and protocol validation remain explicit.
