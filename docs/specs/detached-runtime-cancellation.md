# Detached runtime cancellation

- Read authoritative persisted snapshots when a persistence target exists, otherwise use memory.
- Identify active detached runtime agents by detached=true, worker adapter runtime, background type and a Bash/Pyrun output reference.
- Cancel active descendants in reverse store order. Suppress terminal notifications for identified detached descendants; omit the option for the requested root.
- Before child terminalization, cancel active direct detached runtime children with the cleanup reason and suppression=true, then wait for all active descendants.
- Missing ownership and rejected mutations remain explicit failures; successful cancellation publishes the coordinator snapshot and sends the canonical runtime mailbox command.

[Implementation, proof and open integration](../porting/traceability/detached-runtime-cancellation.md).
