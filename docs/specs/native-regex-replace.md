# Native regex-replace extension

Load the `regex-replace-pi-tool` package as hidden `<native:regex-replace-pi-tool>` only when a user path selects it. Resolve `../target/release/regex-replace-json` from the resolved extension entry directory.

- Register `regex_replace` with the supplied TypeScript tool's exact schema, descriptions, prompt guidance and required approval.
- Serialize operations. Plan first with the execution context cwd, one leading `@` removed from the glob, and limits defaulting to 100 files, 10 MiB input, and 10000 matches. Dry run returns the plan. Apply passes its hash and sorted unique absolute targets while holding the shared file mutation queues for all targets.
- Execute the CLI with a temporary JSON request file as its only argument and the context cwd. Remove that file and its temporary directory on success or failure. Preserve result validation and errors; nonzero exit uses trimmed stderr or `regex-replace-json exited with code N`.
- Reject pre-cancelled execution before planning. Planning receives cancellation; apply must finish once started and does not receive cancellation.
- Preserve the summary, joined diffs, 8 KiB / 1000-line model output limit and truncation notice. Retained JSON details have an 8 KiB limit. Render preview/apply calls, summaries, expanded colored diffs, partial progress and invalid-details errors as in the supplied source.
