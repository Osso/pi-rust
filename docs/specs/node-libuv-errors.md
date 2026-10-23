# Node/libuv errors

- Raw Linux OS error numbers use one shared Node/libuv code and description table, captured from Node 26.11.1 / libuv 1.52.1 (`util.getSystemErrorMap()`, 85 entries).
- Unknown raw error numbers retain Node's exact `Unknown system error <negative-number>` for both code and description. Do not substitute an ErrorKind code or OS description.
- ErrorKind-only synthetic errors retain their explicit compatibility mapping and custom diagnostics without inventing errno.
- Filesystem, stream, spawn and POSIX homedir consumers share this authority. Error records own code strings because unknown numbers produce dynamic names.
- [Traceability](../porting/traceability/node-fs-errors.md) maps behavior to differential proof. The source worktree remains immutable.
