# Resident session file preparation

Reference: unmodified `core/resident-session.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.

- `ResidentRole` permits architect and supervisor. Existing `isResidentSession` ID/path-segment behavior remains unchanged.
- Preparation recursively creates the role directory using Node lexical join semantics, scans only regular canonical/underscore-suffixed role files, and returns directory plus retained file path without creating that file.
- A null-agent role listener with nonempty path and live Pi runtime PID wins only when its lexically resolved dirname exactly equals the resolved role directory and its path ends with `.jsonl`. No realpath or file-existence requirement applies. Its basename is added to the candidate list even if absent or otherwise unmatched.
- Retention precedence: live basename, greatest UTF-16-ordered underscore-suffixed filename, canonical filename. File mtimes have no role. For each nonretained matching file, remove its metadata before unlinking it; failures propagate immediately.
- Afterwards remove missing-file metadata only for the exact role ID or complete slash/backslash role-directory segment, except the exact live path. Existing paths, other roles, and lookalikes remain.
- Filesystem and database errors retain source ordering. Symlinks are not regular candidates; metadata existence checks follow symlinks. Join/resolve are lexical, not canonical filesystem resolution.

- NUL-containing joined filesystem paths must fail before mkdir with source-equivalent TypeError / ERR_INVALID_ARG_VALUE, without syscall/errno fields. Error excerpts preserve Node quoting, control escaping, multiline inspection and the 128-UTF-16-unit limit (including split surrogates).

Scope: fresh-file preparation API only, plus the minimal shared Node path-argument error formatter and source-proven VT-inspection root fix authorized after actual-source mismatches. No external consumer wiring, service orchestration, shared DB/process/path implementation changes, or full-runtime ownership proof.

Proof status and limitations: [module traceability](../porting/traceability/resident-session.md).
