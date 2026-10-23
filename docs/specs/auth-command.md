# Removed auth command

`auth check`, `auth print-api-key` and `auth print-bearer-token` are absent from frozen `4f9e6f8a4`. Q15 removes these old-only commands, parser/help and tests under parent approval (2026-10-23). They are not parity gaps or supported compatibility behavior.

The frozen OAuth login contract is `packages/coding-agent/src/cli/login-command.ts`, using AuthStorage directly. [Removal traceability](../porting/traceability/auth-command.md).
