# Main Runtime Split Design

## Goal

Reduce `src/main.rs` responsibility by moving refresh, update, and route helper logic into focused runtime modules without changing user-visible behavior.

## Scope

This refactor keeps the existing TUI event loop in `src/main.rs` and extracts three responsibilities:

- `src/runtime/refresh.rs`: command capture helpers, `tick_update`, public IP refresh, snapshot construction.
- `src/runtime/update_flow.rs`: automatic/manual GitHub release check, install trigger, update message draining.
- `src/runtime/routes.rs`: route path lookup, route raw source selection, raw viewer command label helper.

The existing `tick_update(app)` call pattern remains available through `lazyifconfig::runtime::refresh::tick_update`. Route and update helpers become crate-visible functions used by `main.rs`.

## Non-Goals

- No UI rendering changes.
- No key binding changes.
- No parser behavior changes.
- No update protocol changes.
- No broad `app.rs` or `ui.rs` split in this slice.

## Architecture

Add `src/runtime/mod.rs` and export focused submodules. `main.rs` becomes the orchestration layer: terminal setup, event loop, and key dispatch. Runtime modules own side-effect-heavy operations that already exist today.

`refresh.rs` depends on collectors, command specs, update flow, and app/model types. It exposes `tick_update(app: &mut App) -> Result<(), String>`.

`update_flow.rs` owns release check/install lifecycle. It exposes:

- `drain_update_messages(app: &mut App)`
- `maybe_start_auto_update_check(app: &mut App)`
- `maybe_start_auto_update_install(app: &mut App)`
- `start_update_check(app: &mut App, manual: bool)`
- `start_update_install(app: &mut App, manual: bool)`

`routes.rs` owns route inspector command helpers. It exposes:

- `run_route_path_lookup(app: &mut App)`
- `routes_raw_sources(app: &App) -> Vec<CommandSourceId>`
- `raw_viewer_command_to_copy(app: &App, src_id: CommandSourceId) -> String`

## Data Flow

The event loop still calls `tick_update` on startup, refresh key, timer tick, and after process kill. `tick_update` drains async update messages, starts automatic update work when needed, runs local command captures, parses data, then replaces the current snapshot.

Manual update keys call `start_update_check` and `start_update_install` from `update_flow.rs`.

Route inspector input still calls `run_route_path_lookup`. Raw route viewer source selection still calls `routes_raw_sources`.

## Error Handling

Existing behavior is preserved:

- Interface command failure still returns `Err` from `tick_update`.
- Route table, default route, IPv6 route, and rule command failures remain non-fatal where they are non-fatal today.
- Update failures continue to produce `UpdateStatus::Error` and timeline events.
- Route path lookup still writes either `latest_path_result` or `latest_path_error`.

## Tests

Existing tests currently embedded in `main.rs` move with extracted helpers:

- Route lookup empty destination test moves to `runtime::routes` tests.
- Route command error label test moves to `runtime::routes` tests.
- Route raw source ordering test moves to `runtime::routes` tests.
- Raw viewer command fallback test moves to `runtime::routes` tests.
- IPv6 route merge test moves to `runtime::refresh` tests.
- `tick_update` smoke test moves to `runtime::refresh` tests.

No new behavior tests are required because this is a structural refactor. Existing integration tests continue to cover app state, parsers, and tools.

## Verification

Primary verification command:

```bash
cargo test
```

Current workspace PowerShell does not have `cargo` in `PATH`, so local verification may be blocked until Rust toolchain is available in this environment.
