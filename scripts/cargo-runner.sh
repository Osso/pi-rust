#!/bin/sh
# Cargo's common test-binary entrypoint (.cargo/config.toml). Keep ordinary
# `cargo run` unchanged; test binaries and every CLI/oracle descendant enter
# the same mandatory helper/network boundary used by bounded startup proofs.
case "$1" in
*/deps/*) ;;
*) exec "$@" ;;
esac
root_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd) || exit 1
exec /usr/bin/python3 "$root_dir/scripts/test-sandbox.py" -- "$@"
