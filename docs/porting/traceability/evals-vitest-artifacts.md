# evals/src/vitest-evals/artifacts.ts

Source: `packages/evals/src/vitest-evals/artifacts.ts` (113 lines). Rust:
`crates/pi-evals/src/vitest_evals/artifacts.rs`.
Tests: `crates/pi-evals/tests/artifacts.rs`, `crates/pi-evals/tests/differential_artifacts.rs`.

## Exported symbols

| TypeScript | Rust | Tests |
|---|---|---|
| `PI_SESSION_SNAPSHOT_ARTIFACT` | same constant (`piSessionJsonl`) | success test |
| `PiSessionAttachment` / `SourceAttachment` | `TestAttachment` | record tests |
| `recordEvalSessionArtifact` | `record_eval_session_artifact` | record/no-op/invalid-metadata tests |
| `recordEvalSourceArtifact` | `record_eval_source_artifact` | record test |
| `persistEvalArtifactReferences` | `persist_eval_artifact_references` | differential + invalid-name test |

## Differential coverage

`differential_artifacts::persisted_artifact_references_match_the_typescript_module` calls the real
TypeScript persistence function in its own temp directory and compares the returned references and
every persisted file body byte-for-byte for: matching run ids, non-matching run ids, empty
attachments, other artifact types, multiple attachments, unicode run ids, and no artifacts.
`invalid_artifact_names_match_the_typescript_error` compares the `TypeError` message for
`nested/hello.ts`.

## Branches

- Session artifact requires `runId` and `piSessionJsonl` strings; otherwise
  `Pi eval session artifact metadata is invalid.` (TypeError).
- Category (`sessions`/`sources`), `sha256(runId)` directory, `basename` validation
  (`Invalid eval artifact name: <name>`), recursive `0700` directory, `0600` file, relative
  `category/hash/name` reference.
- `persist` writes with `std::fs` (the source uses `fs/promises`); the function is synchronous
  because no async resource is involved.

## Gaps

- `recordArtifact(task, ...)` adds a `location` field from the call stack and validates runner
  support; the Rust port records the artifact fields the reporter reads (`type`, `runId`,
  `attachments`). `location` is a Vitest runner responsibility.
- Attachments from other test artifact types (e.g. `internal:annotation`) are not modeled beyond the
  type string; `persist` ignores them by type/run id, as the source does.

## Discrepancies

- None.
