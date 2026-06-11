# UI Module Split Design

## Goal

Reduce `src/ui.rs` size and responsibility by moving focused rendering helpers into `src/ui/` submodules without changing visible TUI behavior.

## Scope

This refactor keeps the public UI entrypoint as `lazyifconfig::ui::draw(frame, app)` and preserves all current layout, styles, labels, keyboard hints, and tests. The split targets helper groups that already have clear boundaries:

- `src/ui/tools.rs`: Tools Hub list/detail rendering and input modal helpers.
- `src/ui/tables.rs`: ports, connections, and routes table rendering plus table row visibility helpers.
- `src/ui/overlays.rs`: help popup, raw output viewer, and release notes viewer.
- `src/ui/details.rs`: ports, connections, and route inspector detail panel line builders.

`src/ui.rs` remains the orchestration layer for top-level layout, header/status/footer helpers, shared formatting helpers, and `draw()`.

## Non-Goals

- No UI design changes.
- No keyboard behavior changes.
- No `App` state changes.
- No parser or runtime changes.
- No full view-by-view rewrite in this slice.

## Architecture

Add module files under `src/ui/` and declare them from `src/ui.rs`. Moved functions use `pub(super)` visibility so they remain private to the `ui` module tree. Shared helper functions that submodules need also become `pub(super)` in `src/ui.rs`.

This preserves the current public API:

```rust
pub fn render_title() -> &'static str
pub fn draw(frame: &mut Frame, app: &App)
```

Module boundaries:

- `tools.rs` owns `render_tools_view`, `render_tools_input_modal`, `tools_input_has_missing_values`, and modal centering helper if only used there.
- `tables.rs` owns `render_ports_table`, `render_connections_table`, `render_routes_table`, header label helpers, `visible_table_rows`, `visible_rows`, `format_endpoint`, and `highlighted_filter_cell`.
- `overlays.rs` owns `draw_help`, `draw_release_notes_viewer`, `draw_raw_viewer`, raw search highlighting helpers, centered rectangle helper, and release note truncation helpers.
- `details.rs` owns route inspector details, section tabs, connection/port/route detail line builders, route graph/diagnostic lines, and local formatting helpers used only by detail panels.

## Data Flow

`draw()` computes the same top-level layout and dispatches to submodule render functions. Submodules receive `Frame`, `App`, and `Rect` exactly like existing helpers, so they do not own app state or mutate state. All rendering remains derived from immutable `&App`.

## Error Handling

Existing rendering behavior is preserved. Empty data, missing selected rows, missing command output, and absent update notes continue to render fallback text through the moved helper functions.

## Testing

Existing `ui.rs` unit tests remain valid and continue to call public `draw()` plus private helpers through the `ui` module test scope. If private helper tests reference moved functions, move those tests into the matching submodule test block or expose the helper as `pub(super)` for the existing test module.

Verification commands:

```bash
cargo test --no-run
cargo test ui::
cargo test
```

Current Windows environment has three baseline OS-command test failures unrelated to UI:

- `command::tests::test_run_ifconfig_success`
- `command::tests::test_run_netstat_ib_success`
- `command::tests::test_run_netstat_success`

This refactor must not add new failures beyond those baseline failures.
