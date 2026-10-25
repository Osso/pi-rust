# Interactive event display

Common-pin `interactive-mode.ts` event handling must preserve these visible results:

- Visible assistant deltas update the assistant tail only. Interleaved tool-call snapshots must not create or update standalone tool rows before the completed assistant message or tool-execution event does so. Completed assistant text stays before its tool rows.
- Failed automatic retry uses `Unknown error` when its final error is absent or empty.
- Provider retries show `provider retry (attempt N)` or `provider retry (N/MAX)`, with ` in SECONDSs` when delay is supplied; seconds are rounded up. Transport fallback instead shows `falling back to TRANSPORT`, in uppercase. Each message ends with `: REASON`.
- An empty compaction error does not emit a failure notification. Manual cancellation still reports `Compaction cancelled`, clears the compaction status, and ends terminal progress.

- Session replay shows tool completion time from the recorded result timestamp, or the recorded failed/aborted assistant timestamp. An empty recorded assistant error uses `Error` for its tool row; aborted records use the frozen abort message.

Proof uses actual frozen and native mode instances with matching event inputs, complete chat/tail/status UTF-16 frames at widths 25 and 120, and terminal-progress calls. These bounded handler tests do not establish whole-mode event or input parity.
