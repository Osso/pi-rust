# SDK console argument inspection

SDK examples use the shared Node IO/argument inspection authority in `pi-js-compat/src/node_fs.rs`, not JSON rendering. [SDK proof](../porting/traceability/sdk-examples.md).

## What it must do

- [ ] Preserve Node's non-colored inspection of SDK string arrays and diagnostic objects: quoting, spacing, compact rows, multiline indentation, depth and truncation.
- [ ] Preserve UTF-16 string units until inspection escapes or the final UTF-8 output boundary.
- [ ] Keep existing filesystem/spawn error inspection on the same formatting authority.

## How it works

- [Shared Node IO mapping](../porting/traceability/node-fs-errors.md).

## Implementation inventory

- `crates/pi-js-compat/src/node_fs.rs`: shared quoting/container/column layout.
- `crates/pi-coding-agent/examples/sdk/02_custom_model.rs`: available-model console list.
- `crates/pi-coding-agent/examples/sdk/04_skills.rs`: skill list and diagnostic objects.

## Tests asserting this spec

- `crates/pi-js-compat/tests/console_inspect.rs`
- `crates/pi-coding-agent/tests/example_sdk_custom_model.rs`
- `crates/pi-coding-agent/tests/example_sdk_skills.rs`

## Known gaps (current cycle)

- [ ] Current default-inspection matrix and SDK complete-output proofs pending.
- [ ] TTY automatic-color behavior is not established by non-TTY fixture output.

## Out of scope

JavaScript functions, symbols and cyclic/non-JSON argument objects are outside this typed helper's input domain.
