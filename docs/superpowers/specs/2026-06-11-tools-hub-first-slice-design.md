# Tools Hub First Slice Design

## Goal

Add a new top-level `Tools(t)` tab that provides a small, safe, on-demand network toolbox inside lazyifconfig. The first slice includes the Tools Hub shell, shared tool architecture, runnable DNS Lookup, Port Check, and Ping tools, plus visible disabled placeholders for Whois Lookup, IP Information, TLS Inspector, and Traceroute.

## Scope

This first pass intentionally does not implement all seven tools. Whois Lookup, IP Information, TLS Inspector, and Traceroute appear in the registry and UI as planned tools, but they are disabled and cannot be executed.

Runnable tools:

- DNS Lookup
- Port Check
- Ping

Planned placeholders:

- Whois Lookup
- IP Information
- TLS Inspector
- Traceroute

## Existing State Constraints

The repository currently has in-flight dirty changes around active connection IP/port splitting. The Tools work must preserve those changes and avoid reverting or reshaping connection behavior. Any edits to shared files such as `src/app.rs`, `src/ui.rs`, and `src/main.rs` must be scoped to adding Tools behavior and, if necessary, completing references to the existing split fields without undoing the split.

Existing tabs must keep their current behavior:

- Interface
- Network
- Port
- Connection
- Route
- Timeline

## Architecture

Tools are modeled through a registry and runner abstraction so future tools can be added as registry entries plus focused implementations.

The shared module owns:

- tool identity and metadata
- runnable versus planned status
- typed input fields
- structured result sections
- raw output text
- execution status

Proposed module layout:

```text
src/tools/mod.rs
src/tools/dns.rs
src/tools/port_check.rs
src/tools/ping.rs
```

Core types:

```rust
pub enum ToolId {
    DnsLookup,
    WhoisLookup,
    IpInformation,
    PortCheck,
    TlsInspector,
    Ping,
    Traceroute,
}

pub enum ToolAvailability {
    Runnable,
    Planned,
}

pub struct ToolDefinition {
    pub id: ToolId,
    pub name: &'static str,
    pub description: &'static str,
    pub availability: ToolAvailability,
    pub fields: &'static [ToolField],
}

pub struct ToolInput {
    pub values: std::collections::BTreeMap<String, String>,
}

pub struct ToolResult {
    pub title: String,
    pub sections: Vec<ToolResultSection>,
    pub raw_output: String,
}

pub struct ToolResultSection {
    pub label: String,
    pub lines: Vec<String>,
}
```

The runner dispatches on `ToolId` through one common async function:

```rust
pub async fn run_tool(id: ToolId, input: ToolInput) -> Result<ToolResult, String>
```

Future tools should be added by extending the registry and adding a focused runner module. They should not require custom one-off UI state or key handling.

## App State

`ViewMode::Tools` is added between `Routes` and `Timeline` in tab order:

```text
Interface(i)
Network(n)
Port(p)
Connection(c)
Route(g)
Tools(t)
Timeline(e)
```

Tools state is isolated inside a dedicated `ToolsState` stored on `App`. It tracks:

- selected tool index
- active input field index
- input values per tool
- whether input editing is active
- current execution status
- last result per tool
- last error per tool
- raw output scroll

This avoids mixing Tools-specific editing and result state into the existing port, connection, route, and raw viewer state.

## Execution Model

Tool execution must never block the TUI event loop. Pressing Enter on a runnable tool starts an async task with `tokio::spawn`. The UI stores a `Running` status immediately and later picks up the finished result from a shared pending-result queue or mutex-backed slot.

The existing event loop continues to handle navigation, rendering, and refresh while a tool is running.

Disabled planned tools do not spawn jobs. Pressing Enter on a planned tool shows or keeps a planned/disabled status.

## Tool Behavior

### DNS Lookup

Input:

- `target`: domain or IP address

Initial execution may shell out for speed and platform coverage. The command selection should be behind the tool interface:

- prefer `dig` if available
- fall back to `host`
- fall back to `nslookup`

Structured result shows resolved record groups when parsing is available. If parsing is limited, the first pass may provide a concise status section plus raw command output. Raw output contains the command display line and captured stdout/stderr.

### Port Check

Input:

- `host`
- `port`

Execution uses native Rust TCP connect with timeout. It does not shell out. The result shows:

- status: OPEN, CLOSED, or ERROR
- latency when a connection succeeds
- timeout/error detail when it fails

Raw output is synthetic because this is a native operation, for example:

```text
$ lazyifconfig tools port-check github.com 443
OPEN in 24ms
```

### Ping

Input:

- `target`

Initial execution may shell out to the platform `ping` command. The command selection should be behind the tool interface and use a small packet count so the command completes promptly.

Structured result shows summary lines such as min, avg, max, and loss when parsed. If parsing is incomplete on a platform, the result still shows command status and keeps the full raw output available.

## UI Design

The Tools view keeps the app's existing two-pane rhythm:

- left pane: tool list
- right pane: selected tool workspace

The left pane lists all seven tools. Runnable tools use normal styling. Planned tools are dimmed and labeled as planned.

The right pane for runnable tools displays:

```text
Input
Results
Raw Output
```

The input area shows one or more fields. `Tab` moves to the next field. Typing edits the active field. `Enter` runs the selected tool when input editing is not active, or accepts the field when editing is active.

The results area displays structured sections. The raw output area shows captured or synthetic raw output and supports scrolling. Full reuse of the existing modal raw viewer is not required in the first slice, but every runnable tool must always retain raw output in state.

The right pane for planned tools displays the description and a clear planned/disabled message. Planned tools are not executable.

## Keyboard Behavior

Global tab navigation remains unchanged, with `t` added for Tools:

- `i`: Interface
- `n`: Network
- `p`: Port
- `c`: Connection
- `g`: Route
- `t`: Tools
- `e`: Timeline

Tools-specific keys:

- `Up` / `Down` or `j` / `k`: select tool when not editing input
- `Tab`: move to next input field
- `Enter`: run runnable selected tool or accept input editing
- `r`: rerun the selected runnable tool
- `/`: focus the first input field
- `Esc`: leave input editing
- `[` / `]`: scroll tool details/raw output

Existing shortcut behavior in other tabs must remain unchanged.

## Error Handling

Tool errors are displayed as structured result sections and in raw output where possible. Missing system commands produce a clear message, such as "No DNS command found: tried dig, host, nslookup." Network timeouts and connection failures are expected outcomes, not app failures.

The TUI must keep running after command failure, invalid input, timeout, or missing command.

## Testing Strategy

Tests are written before implementation.

Coverage should include:

- registry order and runnable/planned flags
- `ViewMode::Tools` tab cycling and direct `t` selection
- Tools navigation and input state transitions
- planned tools cannot run
- Port Check native runner reports open for a local test listener
- Port Check reports closed/error for an unavailable port without hanging
- DNS and Ping command specs are selected through the tool interface
- Tools render smoke test
- existing app state and parser tests continue passing

Because the current worktree has dirty connection IP/port split changes, verification must include the connection-related tests after adapting only the references that still need the split field names.

## Acceptance Criteria

- `Tools(t)` appears in the top tab bar between `Route(g)` and `Timeline(e)`.
- The Tools home view lists DNS Lookup, Port Check, Ping, and planned placeholders for Whois Lookup, IP Information, TLS Inspector, and Traceroute.
- The user can select DNS Lookup, enter a target, run it, and see structured result text plus raw command output.
- The user can select Port Check, enter host and port, run it, and see OPEN/CLOSED/ERROR plus synthetic raw output.
- The user can select Ping, enter a target, run it, and see structured result text plus raw command output.
- Running any first-pass tool does not freeze the TUI event loop.
- Planned placeholder tools are visibly disabled and do not execute.
- Existing Interface, Network, Port, Connection, Route, and Timeline behavior remains unchanged.
