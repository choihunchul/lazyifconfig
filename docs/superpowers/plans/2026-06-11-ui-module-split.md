# UI Module Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split focused rendering helpers out of `src/ui.rs` into `src/ui/` modules without changing visible TUI behavior.

**Architecture:** Keep `src/ui.rs` as the public module and top-level `draw()` orchestrator, then add `tools`, `tables`, `overlays`, and `details` submodules declared inside `ui.rs`. Move existing helper functions mechanically, expose moved functions as `pub(super)`, and expose shared root helpers as `pub(super)` only when submodules need them.

**Tech Stack:** Rust 2021, Ratatui 0.26, Crossterm, existing `App` and model types.

---

## File Structure

- Create `src/ui/tools.rs`: Tools Hub rendering helpers.
- Create `src/ui/tables.rs`: ports, connections, routes table rendering helpers.
- Create `src/ui/overlays.rs`: help, release notes, and raw output overlays.
- Create `src/ui/details.rs`: detail panel rendering helpers and detail-line builders.
- Modify `src/ui.rs`: declare submodules, keep public API, keep shared helpers, import submodule render functions.

---

### Task 1: Add UI Submodule Shells

**Files:**
- Modify: `src/ui.rs`
- Create: `src/ui/tools.rs`
- Create: `src/ui/tables.rs`
- Create: `src/ui/overlays.rs`
- Create: `src/ui/details.rs`

- [ ] **Step 1: Declare submodules**

Add near the top of `src/ui.rs`, after imports:

```rust
mod details;
mod overlays;
mod tables;
mod tools;
```

- [ ] **Step 2: Create empty module files**

Create these files with no functions yet:

```rust
// src/ui/tools.rs
```

```rust
// src/ui/tables.rs
```

```rust
// src/ui/overlays.rs
```

```rust
// src/ui/details.rs
```

- [ ] **Step 3: Compile**

Run:

```bash
cargo test --no-run
```

Expected: compile passes.

---

### Task 2: Move Tools Rendering

**Files:**
- Modify: `src/ui.rs`
- Modify: `src/ui/tools.rs`

- [ ] **Step 1: Move functions**

Move these existing functions from `src/ui.rs` into `src/ui/tools.rs`:

```rust
render_tools_view
render_tools_input_modal
tools_input_has_missing_values
centered_rect
```

Change `render_tools_view` and `render_tools_input_modal` to `pub(super)`.

- [ ] **Step 2: Add imports in `src/ui/tools.rs`**

Use imports matching the moved function bodies:

```rust
use super::*;
```

- [ ] **Step 3: Import render functions in `src/ui.rs`**

Add:

```rust
use tools::{render_tools_input_modal, render_tools_view};
```

- [ ] **Step 4: Compile**

Run:

```bash
cargo test --no-run
```

Expected: compile passes.

---

### Task 3: Move Table Rendering

**Files:**
- Modify: `src/ui.rs`
- Modify: `src/ui/tables.rs`

- [ ] **Step 1: Move functions**

Move these existing functions from `src/ui.rs` into `src/ui/tables.rs`:

```rust
render_ports_table
render_connections_table
port_header_label
connection_header_label
route_header_label
visible_table_rows
visible_rows
render_routes_table
format_endpoint
highlighted_filter_cell
```

Change `render_ports_table`, `render_connections_table`, and `render_routes_table` to `pub(super)`.

- [ ] **Step 2: Add imports in `src/ui/tables.rs`**

Use:

```rust
use super::*;
```

- [ ] **Step 3: Import render functions in `src/ui.rs`**

Add:

```rust
use tables::{render_connections_table, render_ports_table, render_routes_table};
```

- [ ] **Step 4: Compile**

Run:

```bash
cargo test --no-run
```

Expected: compile passes.

---

### Task 4: Move Overlay Rendering

**Files:**
- Modify: `src/ui.rs`
- Modify: `src/ui/overlays.rs`

- [ ] **Step 1: Move functions**

Move these existing functions from `src/ui.rs` into `src/ui/overlays.rs`:

```rust
draw_help
truncate_release_notes
summarize_release_notes_for_banner
draw_release_notes_viewer
get_centered_rect
build_matched_line
draw_raw_viewer
```

Change `draw_help`, `draw_release_notes_viewer`, and `draw_raw_viewer` to `pub(super)`.

- [ ] **Step 2: Add imports in `src/ui/overlays.rs`**

Use:

```rust
use super::*;
```

- [ ] **Step 3: Import overlay functions in `src/ui.rs`**

Add:

```rust
use overlays::{draw_help, draw_raw_viewer, draw_release_notes_viewer};
```

- [ ] **Step 4: Compile**

Run:

```bash
cargo test --no-run
```

Expected: compile passes.

---

### Task 5: Move Detail Rendering

**Files:**
- Modify: `src/ui.rs`
- Modify: `src/ui/details.rs`

- [ ] **Step 1: Move functions**

Move these existing functions from `src/ui.rs` into `src/ui/details.rs`:

```rust
prefix_len_to_ipv4_mask
calculate_ipv4_subnet_u32
calculate_ipv6_subnet_arr
route_family_label
diagnostic_color
render_route_inspector_details
detail_section_tabs
route_inspector_section_tabs
port_details_section_tabs
connection_details_section_tabs
is_remote_connection_target
resolve_connection_interface
connection_summary_lines
connection_whois_lines
port_summary_lines
port_process_lines
route_summary_lines
route_path_lines
vpn_route_lines
route_diagnostic_lines
format_bps
```

Change `render_route_inspector_details`, `connection_summary_lines`, `connection_whois_lines`, `port_summary_lines`, and `port_process_lines` to `pub(super)` if `src/ui.rs` still calls them directly. Keep helpers private if only used inside `details.rs`.

- [ ] **Step 2: Add imports in `src/ui/details.rs`**

Use:

```rust
use super::*;
```

- [ ] **Step 3: Import detail functions in `src/ui.rs`**

Add:

```rust
use details::{
    connection_summary_lines, connection_whois_lines, port_process_lines, port_summary_lines,
    render_route_inspector_details,
};
```

- [ ] **Step 4: Compile**

Run:

```bash
cargo test --no-run
```

Expected: compile passes.

---

### Task 6: Run UI and Full Verification

**Files:**
- Modify: `src/ui.rs`
- Modify: `src/ui/tools.rs`
- Modify: `src/ui/tables.rs`
- Modify: `src/ui/overlays.rs`
- Modify: `src/ui/details.rs`

- [ ] **Step 1: Run targeted UI tests**

Run:

```bash
cargo test ui::
```

Expected: UI tests pass.

- [ ] **Step 2: Run compile check**

Run:

```bash
cargo test --no-run
```

Expected: compile passes.

- [ ] **Step 3: Run full suite**

Run:

```bash
cargo test
```

Expected in this Windows environment: same three baseline failures remain:

```text
command::tests::test_run_ifconfig_success
command::tests::test_run_netstat_ib_success
command::tests::test_run_netstat_success
```

No UI test failures should appear.

- [ ] **Step 4: Review diff scope**

Run:

```bash
git diff --stat
git diff -- src/ui.rs src/ui/tools.rs src/ui/tables.rs src/ui/overlays.rs src/ui/details.rs
```

Expected: diff is move-focused, with no text/layout changes beyond visibility/import/module declarations.

- [ ] **Step 5: Commit**

Run:

```bash
git add -- src/ui.rs src/ui/tools.rs src/ui/tables.rs src/ui/overlays.rs src/ui/details.rs docs/superpowers/plans/2026-06-11-ui-module-split.md
git commit -m "refactor: split ui rendering helpers"
```

---

## Self-Review

- Spec coverage: tools, tables, overlays, and details modules are covered by Tasks 2-5. Public API preservation and verification are covered by Tasks 1 and 6.
- Placeholder scan: no TBD/TODO/fill-in-later items. Function lists are explicit and complete for the chosen split.
- Type consistency: all moved render functions receive existing `Frame`, `App`, `Rect`, `Block`, or model values unchanged; module imports use `use super::*` to preserve access to existing UI types and shared helpers.
