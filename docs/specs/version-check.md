# Version checks

## Contract

- Match frozen `packages/coding-agent/src/utils/version-check.ts` at `4f9e6f8a4`.
- Compare trimmed versions with strict node-semver precedence; invalid versions use trimmed inequality for the newer predicate.
- Every release/version/check entrypoint skips requests when `PI_SKIP_VERSION_CHECK` or `PI_OFFLINE` is nonempty.
- Fetch the latest-version endpoint once with the Pi User-Agent, JSON accept header and a reason-bearing timeout signal (default 10 seconds). Do not add retries or a separate race around a transport that ignores its signal.
- Non-2xx responses and absent/blank version fields yield no release. Trim version, package name and note. Keep existing UTF-16 carriers.
- Direct release/version APIs propagate fetch, body, JSON and timeout errors. Only the automatic newer-version helper catches failures.
- Decode response JSON as Fetch UTF-8, including removal of one leading BOM.
- Self-update errors use the source root message, not an aggregate cause/errno formatter.

## Runtime gap

The native User-Agent truthfully reports `rust` instead of Node/Bun runtime metadata; name, version, platform, architecture, separators and order remain exact (parent classification, 2026-10-25).
