# Session-name v15 oracle

Approved reference: [session-name-source-lock.json](session-name-source-lock.json).
The general [source lock](source-lock.json) remains unchanged.

Prepare once from this checkout, without modifying either reference repository:

```text
git clone --shared --no-checkout /home/osso/Repos/pi target/session-name-oracle
git -C target/session-name-oracle checkout --detach c248b0067c9fc3fa438f967f826c222875c54834
ln -s /home/osso/.worktrees/pi-rust-source-532f7d12/node_modules target/session-name-oracle/node_modules
ln -s /home/osso/.worktrees/pi-rust-source-532f7d12/packages/ai/src/providers/data target/session-name-oracle/packages/ai/src/providers/data
```

`TsOracle::spawn` accepts `session-names:packages/...` for this explicit reference.
It verifies the checkout's full commit SHA, then uses the same process-group teardown
and parent watchdog as every other oracle. DB schema/name differentials compare all
fields in source order; only generated timestamps and fixture paths are substituted.

`cross_implementation_control_db` creates its v15 fixture through real TypeScript,
checks the schema version before Rust opens it, writes a name through Rust, then
reads it back through TypeScript. Legacy v14 SQL fixtures are separate. No test
opens or copies the user's live state/config directories.
