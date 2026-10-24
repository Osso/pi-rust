# Auth contracts

Authority: frozen `packages/ai/src/auth/types.ts:8–182` at4f9e6f8a4.

- Request auth has optional key, headers and base URL. Credentials are tagged API-key or OAuth records; provider env and OAuth extension properties retain native values.
- Credential stores expose only read, serialized modify, and delete. Modify receives the current credential; an absent result leaves it unchanged. No list method or operation cancellation options.
- API-key resolution requires the actual chat/image model, context and optional stored credential. No availability-check interface or resolution signal.
- Auth method names are human display text with exact UTF-16. Optional API-key login is represented by callback presence, never a separate availability flag or an eager invocation.
- OAuth has name, login, refresh and toAuth. No subscription flag, login label or refresh signal parameter.
- Prompts have text/secret/select/manual-code variants and optional cancellation; events have auth_url/device_code/progress variants only. Login callbacks retain whole-flow cancellation.
- Context env/file-existence futures can reject; notify can synchronously throw. Native Result channels preserve those rejections through auth resolution/login, including raw falsy values and UTF-16. Login notifier failures must not become uncaught errors.
- Shared native callback errors preserve thrown values and native error sources. Removed APIs must not survive as compatibility paths.
- DISCREPANCY: frozen Codex browser onAuth runs before its server-close try/finally (:490–493). A throw leaks that listener until process exit; the outer auth wrapper still aborts manual input. Preserve this source bug explicitly.

[Traceability](../porting/traceability/auth-types.md). Native credential-store and callback implementations remain responsible for their own runtime proofs.
