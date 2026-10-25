# Interactive help, changelog and install notices

Match the frozen interactive-mode help/notices handlers. [Proof](../porting/traceability/interactive-mode.md).

## What it must do

- [x] `/hotkeys` renders the complete navigation/editing/other tables and current extension shortcuts, using effective key-display formatting and configured Markdown indentation.
- [x] `/changelog` reads the complete installed changelog on each call, reverses parsed entries, normalizes links and renders the frozen bordered What's New layout. It does not use the cached startup excerpt; an empty file shows `No changelog entries found.`
- [x] Startup selection skips resumed sessions, treats missing/empty last versions as fresh installs and records the current version only for fresh/new entries. Notices run once; absent/empty markdown renders nothing, while full/collapsed and fallback-version output remain exact.
- [x] Install telemetry follows JS truthiness for PI_OFFLINE and the shared telemetry setting/env authority, encodes the version, sends a five-second abort signal and ignores request failures. The approved native `rust` User-Agent runtime token remains explicit.

## Tests asserting this spec

`interactive_help_notices.rs` and `interactive-help-notices-oracle.mts` invoke real frozen handlers with a private package/changelog, real settings/keybindings/Markdown and injected fetch. Native uses isolated real modes. Seventeen cases cover default/custom keys and optional shortcut descriptions, changelog contents, version/notice states and telemetry guards/failure. No live telemetry is sent.

## Boundaries

Windows-specific key text follows the recorded platform gap. Only the approved User-Agent runtime-token field is adapted in the oracle comparison; request URLs, methods, remaining header bytes, frames and settings values remain exact.
