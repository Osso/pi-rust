# MIME file IO

Reference: frozen `coding-agent/src/utils/mime.ts:26–35` and `core/tools/read.ts:81–99` at `4f9e6f8a4`.

- File sniffing is asynchronous: read at most 4100 bytes at position zero without blocking the caller's runtime.
- Read-range operations perform one positioned read, returning the bytes actually read; do not fill a short read with additional reads.
- Open, descriptor-read and close failures preserve their originating Node syscall/error metadata. Descriptor read/close errors do not acquire an open path.
- Close the file after success or read failure. A close failure supersedes a read failure, matching finally.
- Read tools check read access with R_OK before sniffing; metadata existence alone is insufficient.
- Consumers forward the original error message. Neither read nor CLI file processing relabels every sniff failure as open or substitutes Rust's OS display text.

Pure byte-signature detection remains unchanged. [Read traceability](../porting/traceability/read.md), [MIME traceability](../porting/traceability/mime.md), [shared errors](../porting/traceability/node-fs-errors.md).
