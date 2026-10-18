# Traceability: `packages/session-backends/sqlite-node/src/sqlite/storage/lanes.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/storage/lanes.rs`. Tests: conformance,
repository, differential.

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `createInitialLane` | `create_initial_lane` | conformance | |
| `readLanes` | `read_lanes` | repository, differential | Validates `leaf_exists`; `Lane {lane} points at missing entry {leaf}`. |
| `readLane` | `read_lane` | conformance | |
| `readLaneHead` | `read_lane_head` | conformance | Returns the leaf; `invalid_lane`/`storage` errors match. |
| `createLane` | `create_lane` | conformance | Insert + lane move. |
| `moveLane` | `move_lane` | conformance | `changes != 1` → `invalid_lane`. |
| `setLaneLeaf` | `set_lane_leaf` | conformance | |
| `startLaneOperation` | `start_lane_operation` | conformance | Enforces one open operation per lane. |
| `finishLaneOperation` | `finish_lane_operation` | conformance | |
| `readLaneMoveRows` | `read_lane_move_rows` | log-query | |
| `deleteLaneRows` | `delete_lane_rows` | repository | Deletes moves then lanes. |
