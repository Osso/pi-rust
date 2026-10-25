# Configured default HTTP dispatcher

Preserve frozen `core/http-dispatcher.ts` behavior through one [HTTP authority](../porting/traceability/http-dispatcher.md).

## What it must do

- [x] Installing a dispatcher changes subsequent default provider traffic, not only a returned client handle.
- [x] A global settings proxy reaches real provider requests through the process-env overlay.
- [x] Reconfiguration replaces the installed idle timeout; zero disables it.
- [x] Explicit per-request Fetch overrides remain independent.
- [ ] Match EnvHttpProxyAgent lowercase/uppercase precedence, HTTPS-to-HTTP proxy selection, dynamic NO_PROXY host/subdomain/port exclusions and HTTP/1-only selection.

## How it works

- [HTTP trace](../porting/traceability/http-dispatcher.md).

## Implementation inventory

- `crates/pi-ai/src/http.rs`: shared replaceable default Fetch and configured clients.
- `crates/pi-ai/src/http/environment_proxy.rs`: frozen environment-proxy selection.
- `crates/pi-coding-agent-settings/src/core/http_dispatcher.rs`: timeout validation and installation.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/http_dispatcher.rs`: isolated-process real provider/proxy and reconfigured-default idle timeout; existing timeout and helper tests.

## Known gaps (current cycle)

The unchecked proxy-selection branches are source-audited; the parent requested one loopback provider regression, not a new matrix.

## Out of scope

Adversarial input, numeric extremes and Node/Undici runtime faults follow repository approvals.
