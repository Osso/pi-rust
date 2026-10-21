#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

read_source_worktree() {
	node -e 'const fs = require("node:fs"); const lock = JSON.parse(fs.readFileSync("docs/porting/source-lock.json", "utf8")); process.stdout.write(lock.worktree);'
}

typecheck_oracle() {
	node oracle/typecheck-reference.mjs check
	node --test oracle/typecheck-report.test.mjs oracle/typecheck-reference.test.mjs oracle/session-name-reference.test.mjs
	local source_worktree
	source_worktree="$(read_source_worktree)"
	local config
	config="$(mktemp --tmpdir pi-rust-oracles.XXXXXX.json)"
	node oracle/oracle-tsconfig.mjs "$ROOT_DIR/oracle" \
		protocol-oracle.ts rpc-jsonl-oracle.ts truncate-oracle.ts \
		auth-command-oracle.ts config-value-reference-oracle.ts skill-block-oracle.ts \
		default-thinking-level-oracle.ts cli-mode-oracle.ts thinking-level-validator-oracle.ts \
		tool-output-persistence-constants-oracle.ts harness-tools-oracle.ts \
		current-session-version-oracle.ts source-info-oracle.ts \
		coding-agent-format-size-oracle.ts coding-agent-truncation-constants-oracle.ts \
		evals-summary-oracle.ts evals-harness-table-oracle.ts evals-artifacts-oracle.ts \
		evals-pi-harness-oracle.ts >"$config"
	local status=0
	node oracle/typecheck-report.mjs "$source_worktree/node_modules/.bin/tsgo" -p "$config" || status=$?
	rm -f "$config"
	if [ "$status" -ne 0 ]; then
		return "$status"
	fi
	node oracle/public-index-typecheck.mjs
	node oracle/skill-block-typecheck.mjs
	node oracle/summarization-system-prompt-typecheck.mjs
	node oracle/cut-point-result-typecheck.mjs
	node oracle/context-usage-estimate-typecheck.mjs
	node oracle/compaction-details-typecheck.mjs
	node oracle/default-thinking-level-typecheck.mjs
	node oracle/cli-mode-typecheck.mjs
	node oracle/thinking-level-validator-typecheck.mjs
	node oracle/tool-output-persistence-constants-typecheck.mjs
	node oracle/current-session-version-typecheck.mjs
	node oracle/source-info-typecheck.mjs
	node oracle/coding-agent-format-size-typecheck.mjs
	node --check oracle/coding-agent-truncation-constants-typecheck.mjs
	node oracle/coding-agent-truncation-constants-typecheck.mjs
	node oracle/edit-record-typecheck.mjs
}

case "${1:-all}" in
	focused)
		shift
		cargo test -p pi-protocol --lib --test framing --test frame_decoder --test model_cost --test schema_leaves --test model_metadata --test usage --test content_leaves --test session_metadata --test protocol_error_code --test commands --test client_hello --test message_envelopes --test cbor --test codec --test schemas_roundtrip "$@"
		cargo test -p pi-coding-agent --test rpc_jsonl --test auth_command --test config_value_references --test config_value_status --test defaults --test session_tool_output_constants --test current_session_version --test format_size --test truncation_constants --test source_info --test cli_mode --test thinking_level_validator --test skill_block "$@"
		cargo test -p pi-evals --test artifacts --test reporter --test pi_harness --test extensions_eval --test smoke_eval "$@"
		exec cargo test -p pi-agent-core --test truncate --test sanitize_binary_output --test harness_errors --test compaction_errors --test file_info --test queue_mode --test tool_execution_mode --test thinking_level --test agent_tool_execution_context --test before_tool_call_result --test resources --test prompt_templates --test skill_formatting --test image --test edit_line_endings --test fuzzy_match_result --test edit_record --test applied_edits_result --test file_operations --test summary_markers --test bash_execution_message --test branch_summary_message --test compaction_settings --test context_usage_estimate --test compaction_details --test prompt_template_diagnostic --test summarization_system_prompt --test cut_point_result --test edit_diff_full --test harness_env --test harness_tools --test shell_output_capture --test messages_convert --test compaction_utils --test compaction --test branch_summarization "$@"
		;;
	differential)
		shift
		node oracle/session-name-reference.mjs
		cargo test -p pi-protocol --test differential_protocol "$@"
		cargo test -p pi-coding-agent --test differential_rpc_jsonl --test differential_auth_command --test differential_config_value_references --test differential_skill_block --test differential_default_thinking_level --test differential_cli_mode --test differential_thinking_level_validator --test differential_tool_output_persistence_constants --test differential_current_session_version --test differential_source_info --test differential_format_size --test differential_truncation_constants "$@"
		cargo test -p pi-evals --test differential_summary --test differential_harness_table --test differential_artifacts --test differential_pi_harness "$@"
		exec cargo test -p pi-agent-core --test differential_truncate --test differential_harness_tools --test differential_compaction "$@"
		;;
	oracle)
		typecheck_oracle
		;;
	all)
		shift
		node oracle/session-name-reference.mjs
		typecheck_oracle
		exec cargo test --workspace --all-targets "$@"
		;;
	*)
		echo "usage: ./run-tests.sh {focused|differential|oracle|all} [test arguments...]" >&2
		exit 2
		;;
esac
