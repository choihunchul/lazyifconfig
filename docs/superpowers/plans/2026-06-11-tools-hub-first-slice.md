# Tools Hub First Slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first Tools Hub slice with a `Tools(t)` tab, shared tool registry/runner architecture, runnable DNS Lookup, Port Check, and Ping tools, and disabled planned entries for Whois Lookup, IP Information, TLS Inspector, and Traceroute.

**Architecture:** Tools state stays isolated in `ToolsState` on `App`. Tool definitions and execution live under `src/tools`, with a common async `run_tool` entry point. The TUI starts tool jobs with `tokio::spawn` and polls shared completed results so command execution never blocks the event loop.

**Tech Stack:** Rust 2021, ratatui, crossterm, tokio, standard library process and TCP APIs.

---

## File Structure

- Create `src/tools/mod.rs`: shared tool ids, definitions, input/result/status types, registry, dispatch, command specs, result helpers.
- Create `src/tools/dns.rs`: DNS command selection, shell execution, lightweight parsing.
- Create `src/tools/port_check.rs`: native TCP connect with timeout and synthetic raw output.
- Create `src/tools/ping.rs`: platform ping command selection, shell execution, lightweight parsing.
- Modify `src/lib.rs`: export `tools`.
- Modify `src/app.rs`: add `ViewMode::Tools`, `ToolsState`, Tools navigation/input/result methods, tab cycling.
- Modify `src/ui.rs`: add `Tools(t)` tab, Tools status text, Tools left list, selected tool input/results/raw output renderer.
- Modify `src/main.rs`: add key handling for `t`, Tools input/edit/run/rerun actions, async result polling.
- Add `tests/tools_registry.rs`: registry and result-state tests.
- Add `tests/tools_runner.rs`: runner unit/integration tests that avoid external network dependency where possible.
- Update `tests/app_state.rs`: Tools tab navigation/state tests and any connection split references needed by current dirty changes.

## Task 1: Stabilize Existing Dirty Connection Split References

**Files:**
- Modify: `src/ui.rs`
- Modify: `src/main.rs`
- Test: existing tests in `tests/app_state.rs` and `src/ui.rs`

- [ ] **Step 1: Locate stale combined connection fields**

Run:

```bash
rg -n "NavigationItem::Connection \\{[^\\n]*(local|foreign)|\\blocal\\b|\\bforeign\\b" src/ui.rs src/main.rs tests/app_state.rs
```

Expected: Any references to `local` or `foreign` inside `NavigationItem::Connection` are treated as stale because current dirty changes use `local_ip`, `local_port`, `foreign_ip`, and `foreign_port`.

- [ ] **Step 2: Write/confirm failing compile test**

Run:

```bash
cargo test --no-run
```

Expected before fixes if stale references remain: compile failure mentioning missing fields `local` or `foreign` for `NavigationItem::Connection`.

- [ ] **Step 3: Update stale UI/main references only**

Change pattern:

```rust
NavigationItem::Connection {
    proto,
    local,
    foreign,
    state,
    ..
}
```

to:

```rust
NavigationItem::Connection {
    proto,
    local_ip,
    local_port,
    foreign_ip,
    foreign_port,
    state,
    ..
}
```

When display text needs a combined endpoint, build it locally:

```rust
let local = format_endpoint(local_ip, local_port);
let foreign = format_endpoint(foreign_ip, foreign_port);
```

Add a small helper in `src/ui.rs`:

```rust
fn format_endpoint(ip: &str, port: &str) -> String {
    if port.is_empty() || port == "*" {
        ip.to_string()
    } else {
        format!("{ip}:{port}")
    }
}
```

- [ ] **Step 4: Verify compile passes**

Run:

```bash
cargo test --no-run
```

Expected: tests compile. If unrelated system-command tests fail only at runtime later, keep moving to Tools tests and record it during verification.

## Task 2: Add Tool Model and Registry

**Files:**
- Create: `src/tools/mod.rs`
- Modify: `src/lib.rs`
- Test: `tests/tools_registry.rs`

- [ ] **Step 1: Write failing registry tests**

Create `tests/tools_registry.rs`:

```rust
use lazyifconfig::tools::{ToolAvailability, ToolId, ToolRegistry};

#[test]
fn registry_lists_first_slice_tools_in_ui_order() {
    let registry = ToolRegistry::default();
    let ids: Vec<ToolId> = registry.definitions().iter().map(|tool| tool.id).collect();

    assert_eq!(
        ids,
        vec![
            ToolId::DnsLookup,
            ToolId::WhoisLookup,
            ToolId::IpInformation,
            ToolId::PortCheck,
            ToolId::TlsInspector,
            ToolId::Ping,
            ToolId::Traceroute,
        ]
    );
}

#[test]
fn registry_marks_only_first_slice_tools_runnable() {
    let registry = ToolRegistry::default();

    assert_eq!(
        registry.definition(ToolId::DnsLookup).unwrap().availability,
        ToolAvailability::Runnable
    );
    assert_eq!(
        registry.definition(ToolId::PortCheck).unwrap().availability,
        ToolAvailability::Runnable
    );
    assert_eq!(
        registry.definition(ToolId::Ping).unwrap().availability,
        ToolAvailability::Runnable
    );
    assert_eq!(
        registry.definition(ToolId::WhoisLookup).unwrap().availability,
        ToolAvailability::Planned
    );
}
```

- [ ] **Step 2: Run tests to verify red**

Run:

```bash
cargo test --test tools_registry
```

Expected: compile failure because `lazyifconfig::tools` does not exist.

- [ ] **Step 3: Implement minimal registry**

Create `src/tools/mod.rs` with the public model, registry, and stub dispatch:

```rust
use std::collections::BTreeMap;
use std::time::Duration;

pub mod dns;
pub mod ping;
pub mod port_check;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolId {
    DnsLookup,
    WhoisLookup,
    IpInformation,
    PortCheck,
    TlsInspector,
    Ping,
    Traceroute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolAvailability {
    Runnable,
    Planned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolField {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolDefinition {
    pub id: ToolId,
    pub name: &'static str,
    pub description: &'static str,
    pub availability: ToolAvailability,
    pub fields: &'static [ToolField],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolInput {
    pub values: BTreeMap<String, String>,
}

impl ToolInput {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResultSection {
    pub label: String,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResult {
    pub title: String,
    pub sections: Vec<ToolResultSection>,
    pub raw_output: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolExecutionState {
    Idle,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug)]
pub struct ToolRegistry {
    definitions: Vec<ToolDefinition>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            definitions: vec![
                ToolDefinition { id: ToolId::DnsLookup, name: "DNS Lookup", description: "Resolve DNS records for a domain or IP.", availability: ToolAvailability::Runnable, fields: &[ToolField { key: "target", label: "Target", placeholder: "example.com" }] },
                ToolDefinition { id: ToolId::WhoisLookup, name: "Whois Lookup", description: "Look up domain or IP ownership metadata.", availability: ToolAvailability::Planned, fields: &[ToolField { key: "target", label: "Target", placeholder: "github.com" }] },
                ToolDefinition { id: ToolId::IpInformation, name: "IP Information", description: "Summarize ASN, organization, country, and reverse DNS.", availability: ToolAvailability::Planned, fields: &[ToolField { key: "ip", label: "IP", placeholder: "8.8.8.8" }] },
                ToolDefinition { id: ToolId::PortCheck, name: "Port Check", description: "Check TCP connectivity to a host and port.", availability: ToolAvailability::Runnable, fields: &[ToolField { key: "host", label: "Host", placeholder: "github.com" }, ToolField { key: "port", label: "Port", placeholder: "443" }] },
                ToolDefinition { id: ToolId::TlsInspector, name: "TLS Inspector", description: "Inspect certificate and TLS details.", availability: ToolAvailability::Planned, fields: &[ToolField { key: "target", label: "Target", placeholder: "github.com:443" }] },
                ToolDefinition { id: ToolId::Ping, name: "Ping", description: "Measure reachability and latency with ping.", availability: ToolAvailability::Runnable, fields: &[ToolField { key: "target", label: "Target", placeholder: "8.8.8.8" }] },
                ToolDefinition { id: ToolId::Traceroute, name: "Traceroute", description: "Visualize the packet path to a target.", availability: ToolAvailability::Planned, fields: &[ToolField { key: "target", label: "Target", placeholder: "8.8.8.8" }] },
            ],
        }
    }
}

impl ToolRegistry {
    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    pub fn definition(&self, id: ToolId) -> Option<&ToolDefinition> {
        self.definitions.iter().find(|definition| definition.id == id)
    }
}

pub async fn run_tool(id: ToolId, input: ToolInput) -> Result<ToolResult, String> {
    match id {
        ToolId::DnsLookup => dns::run(input).await,
        ToolId::PortCheck => port_check::run(input, Duration::from_secs(3)).await,
        ToolId::Ping => ping::run(input).await,
        _ => Err("This tool is planned and is not executable yet.".to_string()),
    }
}
```

Add to `src/lib.rs`:

```rust
pub mod tools;
```

- [ ] **Step 4: Add empty runner modules**

Create `src/tools/dns.rs`, `src/tools/port_check.rs`, and `src/tools/ping.rs` with temporary errors:

```rust
use super::{ToolInput, ToolResult};

pub async fn run(_input: ToolInput) -> Result<ToolResult, String> {
    Err("not implemented".to_string())
}
```

For `port_check.rs`, use the timeout signature:

```rust
use std::time::Duration;
use super::{ToolInput, ToolResult};

pub async fn run(_input: ToolInput, _timeout: Duration) -> Result<ToolResult, String> {
    Err("not implemented".to_string())
}
```

- [ ] **Step 5: Run registry tests to verify green**

Run:

```bash
cargo test --test tools_registry
```

Expected: registry tests pass.

## Task 3: Add Tools App State

**Files:**
- Modify: `src/app.rs`
- Test: `tests/app_state.rs`

- [ ] **Step 1: Write failing app-state tests**

Append tests to `tests/app_state.rs`:

```rust
#[test]
fn tools_view_is_in_tab_cycle_between_routes_and_timeline() {
    let mut app = lazyifconfig::app::App::default();

    app.set_view_mode(lazyifconfig::app::ViewMode::Routes);
    app.select_next_view_mode();
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Tools);

    app.select_next_view_mode();
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Timeline);
}

#[test]
fn tools_state_selects_only_enabled_registry_entries_normally() {
    let mut app = lazyifconfig::app::App::default();
    app.set_view_mode(lazyifconfig::app::ViewMode::Tools);

    assert_eq!(app.tools.selected_tool_id(), lazyifconfig::tools::ToolId::DnsLookup);
    app.tools.select_next_tool();
    assert_eq!(app.tools.selected_tool_id(), lazyifconfig::tools::ToolId::WhoisLookup);
    assert!(!app.tools.selected_tool_is_runnable());
}
```

- [ ] **Step 2: Run tests to verify red**

Run:

```bash
cargo test --test app_state tools_view_is_in_tab_cycle_between_routes_and_timeline tools_state_selects_only_enabled_registry_entries_normally
```

Expected: compile failure because `ViewMode::Tools` and `App::tools` do not exist.

- [ ] **Step 3: Implement ToolsState and tab mode**

In `src/app.rs`:

```rust
use crate::tools::{ToolAvailability, ToolExecutionState, ToolId, ToolInput, ToolRegistry, ToolResult};

pub enum ViewMode {
    Interface,
    Network,
    Connections,
    Ports,
    Timeline,
    Routes,
    Tools,
}

const VIEW_MODE_TABS: [ViewMode; 7] = [
    ViewMode::Interface,
    ViewMode::Network,
    ViewMode::Ports,
    ViewMode::Connections,
    ViewMode::Routes,
    ViewMode::Tools,
    ViewMode::Timeline,
];
```

Add:

```rust
#[derive(Clone, Debug)]
pub struct ToolsState {
    pub registry: ToolRegistry,
    pub selected_index: usize,
    pub selected_field_index: usize,
    pub editing_input: bool,
    pub inputs: std::collections::HashMap<ToolId, ToolInput>,
    pub results: std::collections::HashMap<ToolId, ToolResult>,
    pub errors: std::collections::HashMap<ToolId, String>,
    pub states: std::collections::HashMap<ToolId, ToolExecutionState>,
    pub raw_scroll: u16,
}

impl Default for ToolsState {
    fn default() -> Self {
        Self {
            registry: ToolRegistry::default(),
            selected_index: 0,
            selected_field_index: 0,
            editing_input: false,
            inputs: std::collections::HashMap::new(),
            results: std::collections::HashMap::new(),
            errors: std::collections::HashMap::new(),
            states: std::collections::HashMap::new(),
            raw_scroll: 0,
        }
    }
}
```

Add methods:

```rust
impl ToolsState {
    pub fn selected_tool_id(&self) -> ToolId {
        self.registry.definitions()[self.selected_index].id
    }

    pub fn selected_definition(&self) -> &crate::tools::ToolDefinition {
        &self.registry.definitions()[self.selected_index]
    }

    pub fn selected_tool_is_runnable(&self) -> bool {
        self.selected_definition().availability == ToolAvailability::Runnable
    }

    pub fn select_next_tool(&mut self) {
        let len = self.registry.definitions().len();
        if len > 0 {
            self.selected_index = (self.selected_index + 1) % len;
            self.selected_field_index = 0;
            self.raw_scroll = 0;
        }
    }

    pub fn select_previous_tool(&mut self) {
        let len = self.registry.definitions().len();
        if len > 0 {
            self.selected_index = if self.selected_index == 0 { len - 1 } else { self.selected_index - 1 };
            self.selected_field_index = 0;
            self.raw_scroll = 0;
        }
    }
}
```

Add `pub tools: ToolsState` to `App` and initialize it in `Default`.

- [ ] **Step 4: Make `update_navigation_items` tolerate Tools**

At the start of `update_navigation_items`, allow Tools with no snapshot:

```rust
if !matches!(self.view_mode, ViewMode::Timeline | ViewMode::Tools) && self.current_snapshot.is_none() {
    self.navigation_items = Vec::new();
    return;
}
```

Add `ViewMode::Tools => { self.navigation_items = Vec::new(); }`.

- [ ] **Step 5: Run app-state tests**

Run:

```bash
cargo test --test app_state tools_view_is_in_tab_cycle_between_routes_and_timeline tools_state_selects_only_enabled_registry_entries_normally
```

Expected: new app-state tests pass.

## Task 4: Implement Port Check Runner

**Files:**
- Modify: `src/tools/port_check.rs`
- Test: `tests/tools_runner.rs`

- [ ] **Step 1: Write failing port check tests**

Create `tests/tools_runner.rs`:

```rust
use std::collections::BTreeMap;
use std::time::Duration;
use lazyifconfig::tools::{port_check, ToolInput};
use tokio::net::TcpListener;

fn input(values: &[(&str, &str)]) -> ToolInput {
    ToolInput {
        values: values
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect::<BTreeMap<_, _>>(),
    }
}

#[tokio::test]
async fn port_check_reports_open_local_listener() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let accept_task = tokio::spawn(async move {
        let _ = listener.accept().await;
    });

    let result = port_check::run(
        input(&[("host", "127.0.0.1"), ("port", &port)]),
        Duration::from_secs(1),
    )
    .await
    .unwrap();

    assert_eq!(result.title, "Port Check");
    assert!(result.sections.iter().any(|section| {
        section.label == "Status" && section.lines.iter().any(|line| line.contains("OPEN"))
    }));
    assert!(result.raw_output.contains("lazyifconfig tools port-check 127.0.0.1"));

    let _ = accept_task.await;
}

#[tokio::test]
async fn port_check_rejects_invalid_port() {
    let err = port_check::run(
        input(&[("host", "127.0.0.1"), ("port", "not-a-port")]),
        Duration::from_millis(50),
    )
    .await
    .unwrap_err();

    assert!(err.contains("Port must be a number"));
}
```

- [ ] **Step 2: Run tests to verify red**

Run:

```bash
cargo test --test tools_runner port_check
```

Expected: tests fail because port check returns `not implemented`.

- [ ] **Step 3: Implement native TCP connect with timeout**

In `src/tools/port_check.rs`:

```rust
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::timeout;
use super::{ToolInput, ToolResult, ToolResultSection};

pub async fn run(input: ToolInput, timeout_duration: Duration) -> Result<ToolResult, String> {
    let host = input.get("host").unwrap_or("").trim();
    let port_raw = input.get("port").unwrap_or("").trim();

    if host.is_empty() {
        return Err("Host is required.".to_string());
    }

    let port: u16 = port_raw
        .parse()
        .map_err(|_| "Port must be a number from 1 to 65535.".to_string())?;
    if port == 0 {
        return Err("Port must be a number from 1 to 65535.".to_string());
    }

    let addr = format!("{host}:{port}");
    let start = Instant::now();
    let connect = timeout(timeout_duration, TcpStream::connect(addr.as_str())).await;
    let elapsed = start.elapsed().as_millis();

    match connect {
        Ok(Ok(_stream)) => Ok(ToolResult {
            title: "Port Check".to_string(),
            sections: vec![
                ToolResultSection { label: "Status".to_string(), lines: vec!["OPEN".to_string()] },
                ToolResultSection { label: "Latency".to_string(), lines: vec![format!("{elapsed}ms")] },
            ],
            raw_output: format!("$ lazyifconfig tools port-check {host} {port}\nOPEN in {elapsed}ms\n"),
        }),
        Ok(Err(err)) => Ok(ToolResult {
            title: "Port Check".to_string(),
            sections: vec![
                ToolResultSection { label: "Status".to_string(), lines: vec!["CLOSED".to_string()] },
                ToolResultSection { label: "Detail".to_string(), lines: vec![err.to_string()] },
            ],
            raw_output: format!("$ lazyifconfig tools port-check {host} {port}\nCLOSED after {elapsed}ms\n{err}\n"),
        }),
        Err(_) => Ok(ToolResult {
            title: "Port Check".to_string(),
            sections: vec![
                ToolResultSection { label: "Status".to_string(), lines: vec!["ERROR".to_string()] },
                ToolResultSection { label: "Detail".to_string(), lines: vec![format!("Timed out after {}ms", timeout_duration.as_millis())] },
            ],
            raw_output: format!("$ lazyifconfig tools port-check {host} {port}\nTIMEOUT after {}ms\n", timeout_duration.as_millis()),
        }),
    }
}
```

- [ ] **Step 4: Run port check tests**

Run:

```bash
cargo test --test tools_runner port_check
```

Expected: port check tests pass.

## Task 5: Implement DNS and Ping Runners Behind Common Interface

**Files:**
- Modify: `src/tools/dns.rs`
- Modify: `src/tools/ping.rs`
- Modify: `src/tools/mod.rs`
- Test: `tests/tools_runner.rs`

- [ ] **Step 1: Write failing command-spec tests**

Append to `tests/tools_runner.rs`:

```rust
#[test]
fn dns_command_candidates_prefer_dig() {
    let candidates = lazyifconfig::tools::dns::command_candidates("example.com");

    assert_eq!(candidates[0].program, "dig");
    assert_eq!(candidates[0].args, vec!["example.com"]);
    assert_eq!(candidates[1].program, "host");
    assert_eq!(candidates[2].program, "nslookup");
}

#[test]
fn ping_command_uses_small_count_per_platform() {
    let mac = lazyifconfig::tools::ping::command_spec_for_os("macos", "8.8.8.8");
    let linux = lazyifconfig::tools::ping::command_spec_for_os("linux", "8.8.8.8");

    assert_eq!(mac.program, "ping");
    assert_eq!(mac.args, vec!["-c", "4", "8.8.8.8"]);
    assert_eq!(linux.program, "ping");
    assert_eq!(linux.args, vec!["-c", "4", "8.8.8.8"]);
}
```

- [ ] **Step 2: Run tests to verify red**

Run:

```bash
cargo test --test tools_runner dns_command_candidates_prefer_dig ping_command_uses_small_count_per_platform
```

Expected: compile failure because command-spec helpers do not exist.

- [ ] **Step 3: Add command spec and async command helper**

In `src/tools/mod.rs`:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCommandSpec {
    pub display: String,
    pub program: String,
    pub args: Vec<String>,
}

pub async fn run_command(spec: &ToolCommandSpec) -> Result<(String, String, Option<i32>), String> {
    let output = tokio::process::Command::new(&spec.program)
        .args(&spec.args)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    Ok((
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code(),
    ))
}
```

- [ ] **Step 4: Implement DNS command candidates and runner**

In `src/tools/dns.rs`, add:

```rust
use super::{run_command, ToolCommandSpec, ToolInput, ToolResult, ToolResultSection};

pub fn command_candidates(target: &str) -> Vec<ToolCommandSpec> {
    vec![
        ToolCommandSpec { display: format!("dig {target}"), program: "dig".to_string(), args: vec![target.to_string()] },
        ToolCommandSpec { display: format!("host {target}"), program: "host".to_string(), args: vec![target.to_string()] },
        ToolCommandSpec { display: format!("nslookup {target}"), program: "nslookup".to_string(), args: vec![target.to_string()] },
    ]
}

pub async fn run(input: ToolInput) -> Result<ToolResult, String> {
    let target = input.get("target").unwrap_or("").trim();
    if target.is_empty() {
        return Err("Target is required.".to_string());
    }

    let mut missing = Vec::new();
    for spec in command_candidates(target) {
        match run_command(&spec).await {
            Ok((stdout, stderr, code)) => {
                let raw_output = format!("$ {}\n{}{}", spec.display, stdout, stderr);
                let mut lines = Vec::new();
                if code == Some(0) {
                    lines.push("Command completed successfully.".to_string());
                } else {
                    lines.push(format!("Command exited with status {:?}.", code));
                }
                for line in stdout.lines().take(8) {
                    if !line.trim().is_empty() {
                        lines.push(line.trim().to_string());
                    }
                }
                return Ok(ToolResult {
                    title: "DNS Lookup".to_string(),
                    sections: vec![ToolResultSection { label: "Result".to_string(), lines }],
                    raw_output,
                });
            }
            Err(err) => missing.push(format!("{} ({})", spec.program, err)),
        }
    }

    Err(format!("No DNS command succeeded. Tried: {}", missing.join(", ")))
}
```

- [ ] **Step 5: Implement Ping command spec and runner**

In `src/tools/ping.rs`, add:

```rust
use super::{run_command, ToolCommandSpec, ToolInput, ToolResult, ToolResultSection};

pub fn command_spec_for_os(_os: &str, target: &str) -> ToolCommandSpec {
    ToolCommandSpec {
        display: format!("ping -c 4 {target}"),
        program: "ping".to_string(),
        args: vec!["-c".to_string(), "4".to_string(), target.to_string()],
    }
}

pub async fn run(input: ToolInput) -> Result<ToolResult, String> {
    let target = input.get("target").unwrap_or("").trim();
    if target.is_empty() {
        return Err("Target is required.".to_string());
    }

    let spec = command_spec_for_os(std::env::consts::OS, target);
    let (stdout, stderr, code) = run_command(&spec).await?;
    let raw_output = format!("$ {}\n{}{}", spec.display, stdout, stderr);
    let mut lines = Vec::new();

    if code == Some(0) {
        lines.push("Ping completed successfully.".to_string());
    } else {
        lines.push(format!("Ping exited with status {:?}.", code));
    }

    for line in stdout.lines().rev().take(4).collect::<Vec<_>>().into_iter().rev() {
        if !line.trim().is_empty() {
            lines.push(line.trim().to_string());
        }
    }

    Ok(ToolResult {
        title: "Ping".to_string(),
        sections: vec![ToolResultSection { label: "Summary".to_string(), lines }],
        raw_output,
    })
}
```

- [ ] **Step 6: Run command-spec tests**

Run:

```bash
cargo test --test tools_runner dns_command_candidates_prefer_dig ping_command_uses_small_count_per_platform
```

Expected: tests pass.

## Task 6: Add Tools UI Rendering

**Files:**
- Modify: `src/ui.rs`
- Test: `src/ui.rs` tests

- [ ] **Step 1: Write failing render smoke test**

Add a test in `src/ui.rs` test module:

```rust
#[test]
fn draw_tools_view_lists_runnable_and_planned_tools() {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    let mut app = App::default();
    app.set_view_mode(ViewMode::Tools);

    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let text = buffer
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();

    assert!(text.contains("Tools(t)"));
    assert!(text.contains("DNS Lookup"));
    assert!(text.contains("Port Check"));
    assert!(text.contains("Ping"));
    assert!(text.contains("Whois Lookup"));
    assert!(text.contains("planned"));
}
```

- [ ] **Step 2: Run test to verify red**

Run:

```bash
cargo test ui::tests::draw_tools_view_lists_runnable_and_planned_tools
```

Expected: compile or assertion failure because Tools UI is not rendered.

- [ ] **Step 3: Add Tools tab and status text**

Update `view_tabs` in `src/ui.rs`:

```rust
(ViewMode::Tools, "Tools(t)"),
```

between Route and Timeline.

Update `get_active_command`:

```rust
ViewMode::Tools => "tool-runner",
```

Update `get_status_text`:

```rust
ViewMode::Tools => " q | t tools | / input | Tab field | Enter run | r rerun | [/] scroll | i/n/p/c/g/e ".to_string(),
```

- [ ] **Step 4: Add Tools-specific renderer branch**

Before rendering the normal left/details panes, branch in `draw`:

```rust
if app.view_mode == ViewMode::Tools {
    render_tools_view(frame, app, top_chunks[0], top_chunks[1]);
} else if app.view_mode == ViewMode::Ports {
    render_ports_table(frame, app, list_block, top_chunks[0]);
} else if app.view_mode == ViewMode::Connections {
    render_connections_table(frame, app, list_block, top_chunks[0]);
} else {
    let list_widget = List::new(list_items).block(list_block);
    frame.render_widget(list_widget, top_chunks[0]);
}
```

Implement `render_tools_view` to render the tool list and details pane from `app.tools`.

- [ ] **Step 5: Run render test**

Run:

```bash
cargo test ui::tests::draw_tools_view_lists_runnable_and_planned_tools
```

Expected: render test passes.

## Task 7: Add Tools Key Handling and Async Job Polling

**Files:**
- Modify: `src/app.rs`
- Modify: `src/main.rs`
- Test: `tests/app_state.rs`

- [ ] **Step 1: Write failing input-state tests**

Append to `tests/app_state.rs`:

```rust
#[test]
fn tools_input_editing_updates_selected_field() {
    let mut app = lazyifconfig::app::App::default();
    app.set_view_mode(lazyifconfig::app::ViewMode::Tools);

    app.tools.start_input_editing();
    app.tools.push_input_char('g');
    app.tools.push_input_char('h');

    let value = app
        .tools
        .input_for_selected_tool()
        .get("target")
        .unwrap()
        .to_string();
    assert_eq!(value, "gh");
}

#[test]
fn planned_tools_are_not_runnable() {
    let mut app = lazyifconfig::app::App::default();
    app.set_view_mode(lazyifconfig::app::ViewMode::Tools);
    app.tools.select_next_tool();

    assert_eq!(app.tools.selected_tool_id(), lazyifconfig::tools::ToolId::WhoisLookup);
    assert!(!app.tools.selected_tool_is_runnable());
}
```

- [ ] **Step 2: Run tests to verify red**

Run:

```bash
cargo test --test app_state tools_input_editing_updates_selected_field planned_tools_are_not_runnable
```

Expected: compile failure because input editing methods do not exist.

- [ ] **Step 3: Implement ToolsState input helpers**

Add methods to `ToolsState`:

```rust
pub fn input_for_selected_tool(&mut self) -> &mut ToolInput {
    let id = self.selected_tool_id();
    self.inputs.entry(id).or_insert_with(|| {
        let mut input = ToolInput::default();
        if let Some(definition) = self.registry.definition(id) {
            for field in definition.fields {
                input.values.insert(field.key.to_string(), String::new());
            }
        }
        input
    })
}

pub fn start_input_editing(&mut self) {
    self.editing_input = true;
}

pub fn stop_input_editing(&mut self) {
    self.editing_input = false;
}

pub fn select_next_field(&mut self) {
    let field_count = self.selected_definition().fields.len();
    if field_count > 0 {
        self.selected_field_index = (self.selected_field_index + 1) % field_count;
    }
}

pub fn push_input_char(&mut self, c: char) {
    let field_key = self.selected_definition().fields[self.selected_field_index].key.to_string();
    self.input_for_selected_tool().values.entry(field_key).or_default().push(c);
}

pub fn pop_input_char(&mut self) {
    let field_key = self.selected_definition().fields[self.selected_field_index].key.to_string();
    if let Some(value) = self.input_for_selected_tool().values.get_mut(&field_key) {
        value.pop();
    }
}
```

- [ ] **Step 4: Add async result queue to App**

Add to `App`:

```rust
pub pending_tool_results: std::sync::Arc<std::sync::Mutex<Vec<(crate::tools::ToolId, Result<crate::tools::ToolResult, String>)>>>,
```

Initialize with an empty `Vec`.

Add methods:

```rust
pub fn drain_pending_tool_results(&mut self) {
    let drained = if let Ok(mut lock) = self.pending_tool_results.lock() {
        lock.drain(..).collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    for (id, result) in drained {
        match result {
            Ok(result) => {
                self.tools.states.insert(id, crate::tools::ToolExecutionState::Succeeded);
                self.tools.errors.remove(&id);
                self.tools.results.insert(id, result);
            }
            Err(error) => {
                self.tools.states.insert(id, crate::tools::ToolExecutionState::Failed);
                self.tools.errors.insert(id, error);
            }
        }
    }
}
```

- [ ] **Step 5: Add key handling in `src/main.rs`**

Add direct tab key:

```rust
KeyCode::Char('t') | KeyCode::Char('ㅅ') => {
    app.help_visible = false;
    app.set_view_mode(ViewMode::Tools);
}
```

Before generic normal-mode handling, add Tools branch when `app.view_mode == ViewMode::Tools`.

Behavior:

- `/`: `app.tools.start_input_editing()`
- `Esc`: `app.tools.stop_input_editing()`
- `Tab`: `app.tools.select_next_field()`
- typing while editing: append to selected field
- `Backspace` while editing: pop selected field
- `j/k` while not editing: select next/previous tool
- `Enter` or `r` while not editing and selected tool runnable: clone input, set state to Running, spawn `lazyifconfig::tools::run_tool`

- [ ] **Step 6: Poll completed tool results**

In the main event loop after existing async message polling or before draw, call:

```rust
app.drain_pending_tool_results();
```

- [ ] **Step 7: Run app-state tests**

Run:

```bash
cargo test --test app_state tools_input_editing_updates_selected_field planned_tools_are_not_runnable
```

Expected: tests pass.

## Task 8: Full Verification

**Files:**
- All touched files

- [ ] **Step 1: Format**

Run:

```bash
cargo fmt
```

Expected: formatting completes with no errors.

- [ ] **Step 2: Run all tests**

Run:

```bash
cargo test
```

Expected: all tests pass. If any test depends on unavailable local system commands, record the exact failure and verify the new unit tests pass.

- [ ] **Step 3: Manual smoke run**

Run:

```bash
cargo run
```

Expected:

- `Tools(t)` appears between `Route(g)` and `Timeline(e)`.
- Pressing `t` enters Tools.
- DNS Lookup, Port Check, Ping are visible as runnable.
- Whois Lookup, IP Information, TLS Inspector, and Traceroute are visible as planned/disabled.
- Editing input and running a tool does not freeze the UI.

## Self-Review Checklist

- Spec coverage: tab, shell, shared architecture, DNS, Port Check, Ping, planned disabled tools, structured result, raw output, async execution, and regression preservation are covered by tasks.
- Placeholder scan: no task depends on "TBD" or unspecified implementation.
- Type consistency: `ToolId`, `ToolInput`, `ToolResult`, `ToolRegistry`, `ToolsState`, and `ViewMode::Tools` names are consistent across tasks.
