# Native user extension identity

Rust loads registered native ports through existing settings extension paths, package manifest entries (`settings.packages` → `package.json` → `pi.extensions`), and auto-discovered extension paths. It does not execute TypeScript.

- Exact resolved-path and original-path factory registrations take precedence.
- Otherwise, inspect at most three directories, starting with the resolved entry file's directory, for the nearest `package.json`. Use its nonempty string `name`; if no usable name exists, use the entry file stem. Look up `<native:IDENTITY>`.
- Register native ports with `ExtensionRegistry::register_hidden("<native:IDENTITY>", factory)`. `load_registered` must never load native ids, including ids registered through `register`.
- Native identity lookup does not replace the extension's original path, resolved path, or source metadata. Listings and diagnostics retain the real user path.
- An unmatched path retains `Extension does not export a valid factory function: <path>` and the existing command exit behavior.
