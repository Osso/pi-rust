# Interactive command dispatch

Interactive input follows frozen `interactive-mode.ts:3969–4196` at `4f9e6f8a4`. [Traceability](../porting/traceability/interactive-mode.md).

## What it must do

- [x] Builtins that accept no arguments consume only the exact trimmed command. An extra argument is forwarded unchanged to normal input handling, not executed as that builtin.
- [x] Only `/model`, `/export`, `/import`, `/cd`, `/name`, `/compact`, and `/login` accept arguments at this dispatcher.
- [x] `/help`, `/thinking`, `/theme`, and `/daxnuts` have no special interactive dispatch; forwarding them does not imply that later session validation accepts them.
- [x] Forwarded input does not create another session or mutate the current session identity.
- [x] The original 19-input sequence retains 18 accepted user inputs and then reports the frozen unknown-command rejection for `/arminsayshi extra`; forwarding is not input loss.

## How it works

- [Frozen branch/proof map](../porting/traceability/interactive-mode.md#exact-command-dispatch)

## Implementation inventory

- `crates/pi-coding-agent/src/modes/interactive/interactive_mode.rs`: submit routing and builtin handlers.

## Tests asserting this spec

- `interactive_mode::ui_tests::unsupported_builtin_arguments_and_old_aliases_are_literal_prompts`: real native mode submission versus actual frozen editor-submit handler; same input callback boundary.
- `oracle/interactive-command-dispatch-oracle.mts`: fresh readonly source receiver per request.
- `interactive_mode::ui_tests::original_literal_command_sequence_matches_frozen_prompt_validation` and `oracle/interactive-command-sequence-oracle.mts`: exact original sequence through source/native prompt validation.

## Known gaps (current cycle)

None in this bounded dispatch/validation slice.

## Out of scope

- Handler-specific UI behavior and session-level unknown-command validation have separate contracts; forwarding is not successful model submission.
