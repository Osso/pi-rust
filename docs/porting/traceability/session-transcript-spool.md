# session-transcript-spool.ts

`withSeekableSessionTranscript` maps to `with_seekable_session_transcript`. Existing plain passes through; archives decode into private TempDir with exclusive0600 output, callback, explicit cleanup. Hidden packaged pi subprocess replaces TS Worker: hard90sec deadline, kill+wait on every early exit, contextual error. Sole path config::get_package_dir()/pi(.exe); existing PI_PACKAGE_DIR supports test layout. No thread fallback or SDK registration. Distribution requires packaged pi colocated with package assets; main owns release packaging proof and early CLI dispatch.

Pending proof: timeout/kill, callback failure, corruption, streaming large record. Fork test originally RED on stale plain File::open ENOENT.
