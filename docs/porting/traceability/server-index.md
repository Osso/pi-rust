# server/index.ts traceability

Source: `packages/server/src/index.ts`. Rust: `crates/pi-server/src/lib.rs`.

Re-exports `errors`, `listener`, `protocol`, `server`, `types`. `snapshots`, `sessions`, `connection`, and transports remain public modules (Rust module paths) but are not flattened into the crate root, except the transport preset used by callers. All re-exported symbols are covered by the module traceability docs.
