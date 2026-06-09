# Active Command Panel Design Spec

**Date:** 2026-06-09  
**Topic:** Active Command Panel  

---

## 1. Goal

Implement a terminal-style, single-line command prompt panel above the "Recent Events" block. This panel transparently displays the primary command that corresponds to the active view mode, reinforcing the `lazyifconfig` identity as a visual frontend for standard Unix commands.

---

## 2. Layout Changes (`src/ui.rs`)

The vertical layout chunks in `src/ui.rs::draw` will be updated from 4 chunks to 5 chunks to allocate a single-row height block for the Active Command Panel:

```rust
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),                    // 0: Top Pane (Main view)
            Constraint::Length(1),                 // 1: Active Command Panel (NEW)
            Constraint::Length(5),                 // 2: Recent Events Panel
            Constraint::Length(filter_bar_height), // 3: Filter Bar
            Constraint::Length(1),                 // 4: Status Bar
        ])
        .split(frame.size());
```

As a result:
- The Event Panel will be rendered on `chunks[2]`.
- The Filter Bar will be rendered on `chunks[3]`.
- The Status Bar will be rendered on `chunks[4]` (updating `status_idx` to `4`).

---

## 3. Command Mapping

A helper function `get_active_command(view_mode: ViewMode) -> &'static str` will be defined to map the active `ViewMode` to the corresponding shell command:

| View Mode | Displayed Shell Command |
| :--- | :--- |
| `ViewMode::Interface` | `ifconfig` |
| `ViewMode::Network` | `ifconfig` |
| `ViewMode::Connections` | `netstat -an` |
| `ViewMode::Ports` | `lsof -iTCP -sTCP:LISTEN -P -n` |
| `ViewMode::Routes` | `netstat -rn` |
| `ViewMode::Timeline` | `event-logger` |

---

## 4. UI Styling & Theme

The panel will render a prompt symbol followed by the active command string, styled to resemble a modern macOS/zsh terminal prompt:

- **Prompt Symbol (`$ `)**: Bold and colored in Terminal Green (`Color::Rgb(0, 255, 102)`).
- **Command Text**: Bold and colored in White (`Color::White`).
- **Background**: Normal terminal background.
- **Example**: `$ ifconfig`

---

## 5. Verification Plan

### Automated Verification
- Run `cargo check` to ensure layout changes and new code compile successfully.
- Run `cargo test` to ensure existing and new unit tests pass.

### Manual Verification
- Run `cargo run`.
- Verify the active command panel is visible right above the "Recent Events" header.
- Switch views (`i`, `n`, `c`, `p`, `e`, `g`) and verify that the command string updates instantly according to the active mode.
