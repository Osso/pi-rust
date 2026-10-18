# cli traceability

Source: `packages/ai/src/cli.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Gap (user decision 2026-10-18): the standalone `pi-ai` package binary (`list`, `login` writing `./auth.json`) is unused by Pi and is not ported. Provider OAuth login is available through the `pi` CLI login command.
