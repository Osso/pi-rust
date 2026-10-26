# Native Pi Atlas

## Source and scope

Port `@mohndoe/pi-atlas` 0.2.1, snapshot `38d84cb549`, from `/home/osso/.worktrees/pi-rust-ext-sources/pi-atlas`. The source and dependency trees are read-only. Reuse native pi-tui, chalk, JavaScript compatibility, and extension APIs. No network requests or live providers are needed.

Register the factory hidden as `<native:@mohndoe/pi-atlas>`. The shared package-name loader owns resolution of `settings.packages` paths and `package.json` identity. Merely registering the factory must not enable the extension.

## Required behavior

- `/atlas` has description `Show Pi Atlas usage dashboard`. Without interactive UI, notify `Stats dashboard requires interactive mode` as an error.
- Resolve the sessions directory as `${XDG_CONFIG_HOME ?? homedir() + '/.config'}/pi/agent/sessions` and the cache as the sibling `pi-atlas-cache.json`. Follow the executable source, not the README's older `~/.pi` paths.
- Show a cancellable loading overlay, with progress, estimated remaining time, and the source overlay geometry. Read the old cache timestamp before loading. Continue with a dashboard after successful loading; preserve source failure notifications and empty-state output.
- Provide Overview, Languages, Models, Projects, Skills, and Usage tabs. Preserve source rendering, colors, chart bucketing, table widths, focus, scroll, marquee, range cycling, resize behavior, close keys, and displayed text. Precompute Today, Last 7 days, Last 30 days, and All time summaries.
- Parse all source-supported session entry types, language edits/writes, tool calls, explicit and implicit skills, provider/model usage, message counts, model changes, thinking levels, and compactions. Preserve source attribution and ordering, including its filter-specific chart behavior.

## Shared persistence

The cache object has `version`, `signature`, `generatedAt`, and `sessions`, in source construction order. Its version is `0.2.1`. The signature is SHA-256 over locale-sorted absolute JSONL paths, byte sizes, and Node-compatible modification milliseconds, each followed by a newline. An empty directory has an empty signature. Reuse requires both current version and current signature. Invalid cache reads are treated as cache misses. Retain the debug force-cache and per-file delay environment variables.

TS-written aggregates must be readable by Rust; Rust-written aggregates must be readable by TS, with identical values and UTF-16 text, including escaped unpaired units. Never project user text into a lossy scalar string.

## Proof and current implementation state

`native_user_extensions/pi_atlas/fixtures/domain.json` was produced by the snapshot in a scratch directory whose `node_modules` points only to `/home/osso/Repos/pi-atlas/node_modules`; the `tsx` executable comes from the common frozen pi checkout. Tests cover frozen parser records, four summary ranges with project/model/provider filters, number/cost/model formatting, cache serialization, nested discovery, and cache invalidation. Local-time summary tests explicitly select UTC for comparison with UTC fixtures.

Implementation is in progress. Loading UI, dashboard components, native registration, package-settings integration proof, and bidirectional cache execution proof remain required; these are not approved deviations. No behavior is waived. The snapshot registers no footer, widget, or event hook outside its `/atlas` custom UI.
