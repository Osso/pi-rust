# Node file URLs

## What it must do

- Preserve Node POSIX `fileURLToPath` output and native error names, messages and properties for the literal `file://` input accepted by coding-agent `normalizePath`.
- Apply WHATWG file-host and pathname states, separators, encoded dot segments, initial drive roots, query/fragment exclusion and strict UTF-8 percent decoding. Retain leading empty pathname segments.
- Convert input to USVString only at the URL boundary; retain the original UTF-16 input in error properties.
- Keep one pathname authority in `pi_js_compat::node_url`. The base caller uses its existing `url::Url` dependency only for host validation/IDNA, never for pathname parsing. No new dependency.
- Classify tested IDNA verdict differences as gaps under main's 2026-10-24 approval; do not add an IDNA implementation.

## Proof

`differential_paths_boundaries` invokes the unchanged frozen paths module, which delegates to actual Node `fileURLToPath`. Compare exact parsed results and native error fields. Initial RED includes multiple-leading-slash loss and invalid cloud-sync argv suppression.

## Exclusions

Windows filesystem path conversion, URL object inputs and runtime argument types outside the typed string surface. This does not implement a general-purpose URL parser.
