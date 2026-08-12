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
		oracle/protocol-oracle.ts
}

case "${1:-all}" in
	focused)
		shift
		exec cargo test -p pi-protocol --lib --test framing --test frame_decoder --test model_cost "$@"
		;;
	differential)
		shift
		exec cargo test -p pi-protocol --test differential_protocol "$@"
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
