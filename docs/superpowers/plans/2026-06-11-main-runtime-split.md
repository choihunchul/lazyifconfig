# Main Runtime Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split refresh, update, and route runtime logic out of `src/main.rs` without changing behavior.

**Architecture:** Add `lazyifconfig::runtime` with focused modules for refresh, update flow, and route helpers. Keep `main.rs` as terminal setup and event-loop orchestration. Move existing unit tests with the functions they cover.

**Tech Stack:** Rust 2021, Tokio, Ratatui, Crossterm, existing collector/command/model modules.

---

## File Structure

- Create `src/runtime/mod.rs`: declares runtime submodules.
- Create `src/runtime/update_flow.rs`: moves update lifecycle helpers from `main.rs`.
- Create `src/runtime/routes.rs`: moves route lookup/raw-view helper functions and tests from `main.rs`.
- Create `src/runtime/refresh.rs`: moves `tick_update`, command capture helpers, public IP refresh, route merge helper, and refresh tests from `main.rs`.
- Modify `src/lib.rs`: exports `runtime`.
- Modify `src/main.rs`: removes extracted functions/tests, imports runtime functions, keeps event loop behavior.

---

### Task 1: Add Runtime Module Shell

**Files:**
- Create: `src/runtime/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create runtime module file**

Create `src/runtime/mod.rs`:

```rust
pub mod refresh;
pub mod routes;
pub mod update_flow;
```

- [ ] **Step 2: Export runtime from library**

Add this line to `src/lib.rs`:

```rust
pub mod runtime;
```

- [ ] **Step 3: Run focused check**

Run:

```bash
cargo test --lib
```

Expected before later tasks: compile fails because module files do not exist yet. Continue to Task 2.

---

### Task 2: Extract Update Flow

**Files:**
- Create: `src/runtime/update_flow.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Move update imports**

Create `src/runtime/update_flow.rs` with imports:

```rust
use crate::app::App;
use crate::command::run_command_capture;
use crate::model::{CommandOutput, CommandSourceId, EventSeverity, NetworkEvent, NetworkEventKind};
use crate::update::{self, CheckOutcome, UpdateMessage, UpdateStatus};
use std::time::Duration;

const RELEASE_CHECK_INTERVAL_SECS: u64 = 6 * 60 * 60;
```

- [ ] **Step 2: Move update functions**

Move these functions from `src/main.rs` into `src/runtime/update_flow.rs` and make them `pub`:

```rust
pub fn maybe_start_auto_update_check(app: &mut App) { /* existing body */ }
pub fn maybe_start_auto_update_install(app: &mut App) { /* existing body */ }
pub fn start_update_check(app: &mut App, manual: bool) { /* existing body */ }
pub fn start_update_install(app: &mut App, manual: bool) { /* existing body */ }
pub fn drain_update_messages(app: &mut App) { /* existing body */ }
```

Use the exact existing bodies from `src/main.rs`. Change paths from `update::...` imports only as needed because this module imports `crate::update`.

- [ ] **Step 3: Import update functions in main**

In `src/main.rs`, remove direct `lazyifconfig::update::{...}` import and `RELEASE_CHECK_INTERVAL_SECS`. Add:

```rust
use lazyifconfig::runtime::update_flow::{start_update_check, start_update_install};
```

Do not import auto-update helpers in `main.rs`; `refresh.rs` will use them.

- [ ] **Step 4: Remove moved functions from main**

Delete the five moved update functions from `src/main.rs`.

- [ ] **Step 5: Run focused compile**

Run:

```bash
cargo test --lib
```

Expected at this point: compile may still fail until Task 4 completes, but no duplicate function definitions remain.

---

### Task 3: Extract Route Helpers

**Files:**
- Create: `src/runtime/routes.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Create route helper module**

Create `src/runtime/routes.rs` with imports:

```rust
use crate::app::App;
use crate::collector::routes::{parse_linux_route_path, parse_macos_route_path};
use crate::command::run_command_capture;
use crate::model::{CommandOutput, CommandSourceId, RouteInspectorSection};
use std::time::SystemTime;
```

- [ ] **Step 2: Move command helper needed by route lookup**

Copy these helper functions into `routes.rs`:

```rust
fn capture_owned_command_output(
    app: &mut App,
    source_id: CommandSourceId,
    command: &crate::command::OwnedCommandSpec,
) -> Result<String, String> {
    let args: Vec<&str> = command.args.iter().map(String::as_str).collect();
    let captured = run_command_capture(command.program.as_str(), &args)?;
    let result = command_stdout(&captured);
    app.command_outputs.insert(
        source_id,
        CommandOutput {
            command: command.display.clone(),
            stdout: captured.stdout,
            stderr: captured.stderr,
            executed_at: std::time::SystemTime::now(),
            exit_code: captured.exit_code,
        },
    );
    result
}

fn command_stdout(output: &crate::command::CommandResult) -> Result<String, String> {
    if output.exit_code == Some(0) {
        Ok(output.stdout.clone())
    } else if output.stderr.trim().is_empty() {
        Err(format!("command exited with {:?}", output.exit_code))
    } else {
        Err(output.stderr.clone())
    }
}
```

- [ ] **Step 3: Move public route helper functions**

Move these functions from `src/main.rs` to `routes.rs`:

```rust
pub fn run_route_path_lookup(app: &mut App) { /* existing body */ }
fn route_path_command_error_message(error: &str) -> String { /* existing body */ }
pub fn routes_raw_sources(app: &App) -> Vec<CommandSourceId> { /* existing body */ }
pub fn raw_viewer_command_to_copy(app: &App, src_id: CommandSourceId) -> String { /* existing body */ }
```

- [ ] **Step 4: Move route tests**

Move these tests from `src/main.rs` into `routes.rs` under `#[cfg(test)] mod tests`:

```rust
route_path_lookup_requires_destination
route_path_command_error_message_uses_literal_destination_label
routes_raw_sources_include_available_optional_outputs_in_order
raw_viewer_command_to_copy_prefers_captured_command_and_falls_back_to_source_label
```

Import needed test types inside the test module:

```rust
use super::*;
use crate::model::{CommandOutput, RoutePathResult};
use std::time::SystemTime;
```

- [ ] **Step 5: Import route functions in main**

In `src/main.rs`, add:

```rust
use lazyifconfig::runtime::routes::{
    raw_viewer_command_to_copy, routes_raw_sources, run_route_path_lookup,
};
```

Remove route helper functions and moved tests from `src/main.rs`.

- [ ] **Step 6: Run route tests**

Run:

```bash
cargo test runtime::routes
```

Expected: route helper tests pass once compilation succeeds.

---

### Task 4: Extract Refresh

**Files:**
- Create: `src/runtime/refresh.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Create refresh module imports**

Create `src/runtime/refresh.rs` with imports:

```rust
use crate::app::App;
use crate::collector::connections::parse_connections;
use crate::collector::interface::{merge_gateways, parse_interfaces};
use crate::collector::ports::parse_listening_ports;
use crate::collector::routes::parse_routes;
use crate::collector::stats::merge_stats;
use crate::collector::system::collect_process_metrics;
use crate::command::{
    default_route_command_spec, interface_command_spec, listening_ports_command_spec,
    route_table_command_spec, run_command_capture, run_netstat_ib,
};
use crate::model::{
    CommandOutput, CommandSourceId, EventSeverity, NetworkEvent, NetworkEventKind, NetworkSnapshot,
    PublicIpInfo,
};
use crate::runtime::update_flow::{
    drain_update_messages, maybe_start_auto_update_check, maybe_start_auto_update_install,
};
use std::time::{SystemTime, UNIX_EPOCH};
```

- [ ] **Step 2: Move refresh functions**

Move these functions from `src/main.rs` into `refresh.rs`:

```rust
pub fn tick_update(app: &mut App) -> Result<(), String> { /* existing body */ }
fn capture_command_output(...) -> Result<String, String> { /* existing body */ }
fn capture_owned_command_output(...) -> Result<String, String> { /* existing body */ }
fn command_stdout(...) -> Result<String, String> { /* existing body */ }
fn merge_additional_route_output(...) -> Vec<crate::model::RouteEntry> { /* existing body */ }
```

Keep bodies behavior-identical. Use `crate::...` paths instead of `lazyifconfig::...` paths.

- [ ] **Step 3: Move refresh tests**

Move these tests from `src/main.rs` into `refresh.rs`:

```rust
additional_linux_ipv6_route_output_is_merged_into_snapshot_routes
test_tick_update
```

Import needed test items:

```rust
use super::*;
use crate::collector::routes::parse_routes;
use crate::model::RouteFamily;
```

- [ ] **Step 4: Import tick_update in main**

In `src/main.rs`, remove collector/parser/refresh-only imports now used only by `refresh.rs`. Add:

```rust
use lazyifconfig::runtime::refresh::tick_update;
```

- [ ] **Step 5: Run refresh tests**

Run:

```bash
cargo test runtime::refresh
```

Expected: refresh tests pass on a host with required network commands. On Windows or missing command hosts, `test_tick_update` may fail due existing OS-command dependency.

---

### Task 5: Clean Main Imports and Full Verification

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Remove unused imports**

Run:

```bash
cargo test --no-run
```

Fix compiler-reported unused imports in `src/main.rs`, `src/runtime/refresh.rs`, `src/runtime/routes.rs`, and `src/runtime/update_flow.rs`.

- [ ] **Step 2: Run formatter**

Run:

```bash
cargo fmt
```

- [ ] **Step 3: Run full test suite**

Run:

```bash
cargo test
```

Expected: tests pass in an environment with Rust and required platform commands. If local environment lacks `cargo`, record that exact blocker.

- [ ] **Step 4: Inspect diff**

Run:

```bash
git diff --stat
git diff -- src/main.rs src/lib.rs src/runtime/mod.rs src/runtime/refresh.rs src/runtime/routes.rs src/runtime/update_flow.rs
```

Confirm diff is move-focused and no keymap/user-visible behavior changed.

- [ ] **Step 5: Commit implementation**

Run:

```bash
git add -- src/main.rs src/lib.rs src/runtime/mod.rs src/runtime/refresh.rs src/runtime/routes.rs src/runtime/update_flow.rs docs/superpowers/plans/2026-06-11-main-runtime-split.md
git commit -m "refactor: split main runtime logic"
```

---

## Self-Review

- Spec coverage: refresh extraction covered by Task 4, update extraction by Task 2, route helper extraction by Task 3, module export by Task 1, verification by Task 5.
- Placeholder scan: no TBD/TODO/fill-in-later entries remain; "existing body" means literal move from existing code, not new behavior design.
- Type consistency: all new paths use `crate::runtime::*` internally and `lazyifconfig::runtime::*` from `main.rs`.
