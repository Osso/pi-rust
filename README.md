# pi-rust

Systematic Rust port of the Pi Agent Harness.

The TypeScript source oracle is pinned in [`docs/porting/source-lock.json`](docs/porting/source-lock.json). Each slice preserves representable observable behavior through source traceability, characterization tests, and differential execution against the real TypeScript implementation.

Current coverage: `@earendil-works/pi-protocol` binary framing, incremental decoding, and typed schemas for models, usage, content leaves, session metadata, commands, client handshakes, phases, references, and protocol error codes.

## Commands

```text
./run-tests.sh focused
./run-tests.sh differential
./run-tests.sh oracle
./run-tests.sh all
cargo fmt --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Status: pre-alpha. No complete Pi-compatible executable exists yet.
