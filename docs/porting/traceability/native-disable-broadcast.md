# Native disable-broadcast

Source: `/home/osso/.worktrees/pi-rust-ext-sources/disable-broadcast/disable-broadcast.ts` (user-supplied snapshot).
Rust: `crates/pi-coding-agent-extensions/src/native_user_extensions/disable_broadcast.rs`.
Contract: [disable broadcast](../../specs/disable-broadcast.md).

Whole-module audit: registration installs one `session_start` callback; each event reads the current active tools and removes all exact `broadcast` names, preserving order and every other name. Hidden registration is wired through `register_small_extensions`; no user path means no activation.

Proof: `native_disable_broadcast_filters_only_broadcast_on_session_start` failed RED on missing factory, then passes GREEN through actual path loading and runner event dispatch. It checks no load-time mutation, duplicate removal, similar names retained, order, and live list changes on the second event.

Checkpoint: required workspace all-target check, fmt check, tools all-target clippy and additional extensions-library clippy pass. Source inventory remains 686 rows / 186679 lines. Changed Rust readability audit has no outstanding findings.
