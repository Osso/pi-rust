#!/usr/bin/env bash
set -euo pipefail

case "${1:-all}" in
	focused)
		shift
		exec cargo test -p pi-protocol --test framing "$@"
		;;
	differential)
		shift
		exec cargo test -p pi-protocol --test differential_framing "$@"
		;;
	all)
		shift
		exec cargo test --workspace --all-targets "$@"
		;;
	*)
		echo "usage: ./run-tests.sh {focused|differential|all} [test arguments...]" >&2
		exit 2
		;;
esac
