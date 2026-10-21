# session-transcript.ts → session_transcript.rs

Frozen 4f9e6f8a4. Exports: isSessionTranscriptFile, readSessionTranscriptPath, readSessionTranscriptBytes, readSessionTranscriptText, createSessionTranscriptReadStream.

`repin_session_storage::frozen_archive_stale_path_never_allocates_identity_or_masks_history`: public TS manager creates and persists, public archive storage compresses; Rust header/load sees original identity; mutable open refuses stale archive without masking. Initial RED: Rust header None. Whole-module proof remains todo: stream corruption, UTF-8 boundaries, predicates/plain precedence and resource bounds.
