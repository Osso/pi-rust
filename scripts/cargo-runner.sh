#!/bin/sh
# Cargo runner for every target binary (.cargo/config.toml).
#
# Test binaries (target/<profile>/deps/*) get a private PI_CODING_AGENT_STATE_DIR,
# like the TypeScript vitest global setup (packages/coding-agent/test/global-setup.ts),
# so tests never open the real ~/.local/state/pi/control.sqlite. Other binaries
# (`cargo run`) run unchanged. The runner execs, so no wrapper process outlives a
# killed test; state directories older than a day are pruned instead of removed
# on exit. They live under TMPDIR because Unix socket paths derived from the
# control database must stay under 108 bytes.
case "$1" in
*/deps/*) ;;
*) exec "$@" ;;
esac
tmp_root=${TMPDIR:-/tmp}
find "$tmp_root" -maxdepth 1 -name 'pi-cargo-test-state-*' -type d -mmin +1440 -exec rm -rf {} + 2>/dev/null
state_dir=$(mktemp -d "$tmp_root/pi-cargo-test-state-XXXXXX") || exit 1
PI_CODING_AGENT_STATE_DIR=$state_dir exec "$@"
