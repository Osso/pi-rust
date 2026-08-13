#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

read_source_worktree() {
	node -e 'const fs = require("node:fs"); const lock = JSON.parse(fs.readFileSync("docs/porting/source-lock.json", "utf8")); process.stdout.write(lock.worktree);'
}

typecheck_oracle() {
	local source_worktree
	source_worktree="$(read_source_worktree)"
	"$source_worktree/node_modules/.bin/tsgo" \
		--noEmit \
		--strict \
		--target ES2022 \
		--module NodeNext \
		--moduleResolution NodeNext \
		--types node \
		--typeRoots "$source_worktree/node_modules/@types" \
		oracle/protocol-oracle.ts \
		oracle/rpc-jsonl-oracle.ts \
		oracle/truncate-oracle.ts \
		oracle/auth-command-oracle.ts \
		oracle/config-value-reference-oracle.ts
	node oracle/public-index-typecheck.mjs
}

case "${1:-all}" in
	focused)
		shift
		cargo test -p pi-protocol --lib --test framing --test frame_decoder --test model_cost --test schema_leaves --test model_metadata --test usage --test content_leaves --test session_metadata --test protocol_error_code --test commands --test client_hello --test message_envelopes "$@"
		cargo test -p pi-coding-agent --test rpc_jsonl --test auth_command --test config_value_references --test config_value_status "$@"
		exec cargo test -p pi-agent-core --test truncate --test sanitize_binary_output --test harness_errors --test compaction_errors --test file_info --test resources --test prompt_templates --test skill_formatting --test image --test edit_line_endings --test file_operations "$@"
		;;
	differential)
		shift
		cargo test -p pi-protocol --test differential_protocol "$@"
		cargo test -p pi-coding-agent --test differential_rpc_jsonl --test differential_auth_command --test differential_config_value_references "$@"
		exec cargo test -p pi-agent-core --test differential_truncate "$@"
		;;
	oracle)
		typecheck_oracle
		;;
	all)
		shift
		typecheck_oracle
		exec cargo test --workspace --all-targets "$@"
		;;
	*)
		echo "usage: ./run-tests.sh {focused|differential|oracle|all} [test arguments...]" >&2
		exit 2
		;;
esac
