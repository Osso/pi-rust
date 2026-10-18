# coding-agent session-health traceability

Pinned source: `packages/coding-agent/src/core/session-health.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `27008` runs the pinned module and the Rust port over the same fixtures: empty/sticky/eligible records, `endSessionHealth`, three `advanceGenerationForPid` transitions (new pid, same pid, zero generation), ten `parseGoalObjective` values, and 42 `deriveLiveStatus` timestamp cases covering UTC, offsets, `Z`-less date-only, `Date.parse` normalization (Feb 31 → Mar 3), hour 24, invalid fields, lowercase `t`/`z`, extended years, and RFC-2822/free-form strings. Focused tests: `crates/pi-coding-agent/tests/session_health.rs` and `crates/pi-coding-agent/tests/iso_time.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `SessionCheckStatus` | `ok \| dead \| timeout \| never` | `enum SessionCheckStatus`; `serializes_check_status_wire_names` |
| `SessionLiveStatus` | `running \| idle \| ended` | `enum SessionLiveStatus` |
| `SESSION_CHECK_STALE_MS` | `5 * 60 * 1000` | `SESSION_CHECK_STALE_MS` |
| `SessionHealthRecord` | camelCase record with nullable pid/timestamps/latency | `SessionHealthRecord` with `Option` fields; `builds_empty_health_records`, differential `27008` |
| `SessionDirectoryEntry` | camelCase directory row | `SessionDirectoryEntry` |
| `SessionBroadcastResult` | camelCase row with `outcome` union | `SessionBroadcastResult` + `SessionBroadcastOutcome` (`snake_case` wire names) |
| `emptySessionHealth` | Zeroed record, `checkStatus: "never"`, `updatedAt = nowIso` | `empty_session_health`; differential `27008` |
| `isStickyDead` | `dead` and `checkedGeneration === agentGeneration` | `is_sticky_dead` (also requires `checkedGeneration` to be present); differential `27008` |
| `isSessionEligibleToReceive` | `ok` and not sticky dead | `is_session_eligible_to_receive`; differential `27008` |
| `deriveLiveStatus` | `dead`/null pid/non-ok → ended; parseable and `now - lastActive <= staleMs` → running; else idle | `derive_live_status`; focused boundary/offset tests, differential `27008` |
| `parseGoalObjective` | Trim; JSON object `objective` string wins when non-empty; otherwise the trimmed payload; falsy/empty → null | `parse_goal_objective(Option<&Value>)`; focused cases, differential `27008` |
| `endSessionHealth` | Clears pid, marks dead for the current generation, zero latency, updates timestamps | `end_session_health`; differential `27008` |
| `advanceGenerationForPid` | Same pid with generation > 0 keeps the generation; otherwise increments; always refreshes activity/updated timestamps | `advance_generation_for_pid`; focused and differential cases |

## Date parsing boundary

`Date.parse` supports far more than ISO 8601. `utils::iso_time::parse_javascript_date` implements the ECMAScript date-time string format — including `T`/space separators, optional seconds and fraction, `Z`/`±HH:MM`/`±HHMM` offsets, extended years, `MakeDay` normalization, and `24:00` — which covers every value the store produces (`toISOString`).

Gap: RFC 2822 (`Thu, 01 Jan 2026 00:00:00 GMT`) and V8's free-form fallback (`Jan 1 2026`) are **not** parsed and yield "ended"/"idle" status. The source's own data never produces them.

## Discrepancies

- `Date.parse` returns `NaN` for unparseable values, which `deriveLiveStatus` treats as idle; the Rust parser returns `None` for the same inputs. No behavior change.
- Offset-less date-times resolve local time through `jiff`'s system time zone (tzdata) instead of V8's zone rules; both consult the host zone and the differential case runs in the same zone.

## Proof inventory

- Focused cases: 10 (session health) + 4 (iso_time).
- Differential request: `27008`, 42 live-status cases plus record/goal fixtures.
