# Interactive input actions

Frozen `interactive-mode.ts` defines these visible input results:

- Ctrl+C clears the editor unless the previous Ctrl+C was less than 500ms ago. Only that shorter interval requests shutdown. Update the remembered press time after successful clearing.
- Thinking visibility toggles its saved setting, rebuilds restored messages, and updates the live assistant tail without discarding it. Report `Thinking blocks: hidden` or `Thinking blocks: visible`.
- Queue restoration clears session and compaction queues. Restore session steering followed by compaction steering, then session follow-ups followed by compaction follow-ups. Join queued input before current editor input with double newlines; omit whitespace-only combined sections without trimming retained text.
- Dequeue reports `No queued messages to restore`, `Restored 1 queued message to editor`, or `Restored N queued messages to editor`. Clear the pending-message display even when no queued input remains.

## Escape and compaction delivery

Escape first cancels a summary edit, selected child turn or Supervisor review. Otherwise it interrupts streaming with preserved queued input, aborts running bash, or clears bash-mode text. Nonempty ordinary editor input stays unchanged. Two presses on an empty editor within500ms use the configured tree/fork action; `none` does nothing. Bash-mode changes refresh the editor border after the editor callback releases its borrow.

Compaction queue delivery preserves extension-command ordering and steering/follow-up mode. The first ordinary prompt starts without blocking delivery of later queued messages. Retry turns receive queued messages through the matching session queue API. A rejected delivery clears partial session queues, restores the original compaction input and previews, and reports `Failed to send queued message(s): REASON` with correct singular/plural spelling.

## Follow-up submission

Alt+Enter reads expanded, trimmed editor text and ignores empty input. During compaction, extension commands execute immediately and other text queues as follow-up input. During streaming, submit through the session prompt API with follow-up behavior and show prompt failures. While idle, use the installed editor submit callback, not the session follow-up queue; clear the editor only if that callback exists. Custom editors inherit the default submit callback.

Proof compares actual frozen/native modes, saved and live assistant content, complete UTF-16 frames at widths25/120, editor text, pending previews and settings. The Ctrl+C case uses two actual presses750ms apart. Other editor/submission/cancellation paths remain under audit.
