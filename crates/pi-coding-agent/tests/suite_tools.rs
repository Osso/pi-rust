// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "ask_questions_tool.rs"]
mod ask_questions_tool;
#[path = "bash_executor.rs"]
mod bash_executor;
#[path = "bash_stdin.rs"]
mod bash_stdin;
#[path = "bash_tool.rs"]
mod bash_tool;
#[path = "bash_tool_detach.rs"]
mod bash_tool_detach;
#[path = "clipboard_temp_files.rs"]
mod clipboard_temp_files;
#[path = "code_index_tools.rs"]
mod code_index_tools;
#[path = "codex_hosted_tools.rs"]
mod codex_hosted_tools;
#[path = "differential_bash_adopt.rs"]
mod differential_bash_adopt;
#[path = "differential_bash_executor_stream.rs"]
mod differential_bash_executor_stream;
#[path = "differential_bash_hook_chunk_errors.rs"]
mod differential_bash_hook_chunk_errors;
#[path = "differential_bash_local.rs"]
mod differential_bash_local;
#[path = "differential_bash_manifest_numbers.rs"]
mod differential_bash_manifest_numbers;
#[path = "differential_bash_restored.rs"]
mod differential_bash_restored;
#[path = "differential_bash_tool.rs"]
mod differential_bash_tool;
#[path = "differential_claude_bash_hook.rs"]
mod differential_claude_bash_hook;
#[path = "differential_code_index_tools.rs"]
mod differential_code_index_tools;
#[path = "differential_codex_hosted_tools.rs"]
mod differential_codex_hosted_tools;
#[path = "differential_file_processor_mime_errors.rs"]
mod differential_file_processor_mime_errors;
#[path = "differential_find_abort.rs"]
mod differential_find_abort;
#[path = "differential_find_numbers.rs"]
mod differential_find_numbers;
#[path = "differential_find_paths.rs"]
mod differential_find_paths;
#[path = "differential_find_tool.rs"]
mod differential_find_tool;
#[path = "differential_grep_tool.rs"]
mod differential_grep_tool;
#[path = "differential_list_tools.rs"]
mod differential_list_tools;
#[path = "differential_ls_filesystem.rs"]
mod differential_ls_filesystem;
#[path = "differential_ls_tool.rs"]
mod differential_ls_tool;
#[path = "differential_mime_file_errors.rs"]
mod differential_mime_file_errors;
#[path = "differential_read_abort.rs"]
mod differential_read_abort;
#[path = "differential_read_config_root.rs"]
mod differential_read_config_root;
#[path = "differential_read_filesystem.rs"]
mod differential_read_filesystem;
#[path = "differential_read_model_note.rs"]
mod differential_read_model_note;
#[path = "differential_read_numbers.rs"]
mod differential_read_numbers;
#[path = "differential_read_tool.rs"]
mod differential_read_tool;
#[path = "differential_tool_detach_registry.rs"]
mod differential_tool_detach_registry;
#[path = "differential_tool_execution.rs"]
mod differential_tool_execution;
#[path = "differential_tool_output_persistence_constants.rs"]
mod differential_tool_output_persistence_constants;
#[path = "differential_write_filesystem.rs"]
mod differential_write_filesystem;
#[path = "differential_write_queue.rs"]
mod differential_write_queue;
#[path = "differential_write_result_boundary.rs"]
mod differential_write_result_boundary;
#[path = "end_turn_tool.rs"]
mod end_turn_tool;
#[path = "file_mutation_queue.rs"]
mod file_mutation_queue;
#[path = "file_processor.rs"]
mod file_processor;
#[path = "find_tool.rs"]
mod find_tool;
#[path = "lockfile.rs"]
mod lockfile;
#[path = "lockfile_cross_process.rs"]
mod lockfile_cross_process;
#[path = "lossless_bash.rs"]
mod lossless_bash;
#[path = "lossless_bash_executor.rs"]
mod lossless_bash_executor;
#[path = "lossless_bash_request_id.rs"]
mod lossless_bash_request_id;
#[path = "lossless_file_processor.rs"]
mod lossless_file_processor;
#[path = "lossless_tool_output_spill.rs"]
mod lossless_tool_output_spill;
#[path = "lossless_tool_paths.rs"]
mod lossless_tool_paths;
#[path = "lossless_tools.rs"]
mod lossless_tools;
#[path = "ls_tool.rs"]
mod ls_tool;
#[path = "read_tool.rs"]
mod read_tool;
#[path = "rpc_jsonl_reader.rs"]
mod rpc_jsonl_reader;
#[path = "tool_definition_wrapper.rs"]
mod tool_definition_wrapper;
#[path = "tool_detach_registry.rs"]
mod tool_detach_registry;
#[path = "tool_detach_snapshot.rs"]
mod tool_detach_snapshot;
#[path = "tool_result_images.rs"]
mod tool_result_images;
