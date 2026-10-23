# Coding-agent system prompt

Baseline: frozen `4f9e6f8a4`, core/system-prompt.ts.

Custom/default prompts preserve native UTF-16 and append, in order: nonempty appended prompt, project context, nonempty user rules, read-enabled skills, local Current date and slash-normalized cwd. Neither branch adds a final newline. Date uses unpadded year and two-digit month/day.

Default prompts show only selected tools with truthy snippets; deduplicated guidelines retain source insertion order, including the spawn_agent+pyrun_eval bridge guidance. Empty append/snippet strings are false, not whitespace-trimmed. Documentation text/paths match the common pin; post-pin environment-variable guidance is absent.

A Rust-only injected clock controls tests without changing production wall-clock behavior. Both runtimes receive identical fixed milliseconds/time zones; prompt output is never date-normalized.
