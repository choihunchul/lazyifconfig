# Active Command Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a terminal-themed Active Command Panel above the "Recent Events" block to dynamically display the primary command corresponding to the selected view mode.

**Architecture:** Update the main vertical layout in `ui.rs::draw` to split the screen into 5 chunks instead of 4. Introduce a mapping function from `ViewMode` to command string, and draw it on the new layout chunk in terminal prompt styling.

**Tech Stack:** Rust, Ratatui

---

## File Structure & Proposed Changes

### [lazyifconfig]

#### [MODIFY] [ui.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/ui.rs)
Modify to update layout division, add view-to-command mapping helper, and render the command line.

---

## Detailed Tasks

### Task 1: View Mode Command Mapping

**Files:**
- Modify: [src/ui.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/ui.rs)

- [ ] **Step 1: Write helper function and tests in `src/ui.rs`**
  Add `get_active_command` and its unit tests under `mod tests`:
  ```rust
  fn get_active_command(view_mode: ViewMode) -> &'static str {
      match view_mode {
          ViewMode::Interface | ViewMode::Network => "ifconfig",
          ViewMode::Connections => "netstat -an",
          ViewMode::Ports => "lsof -iTCP -sTCP:LISTEN -P -n",
          ViewMode::Routes => "netstat -rn",
          ViewMode::Timeline => "event-logger",
      }
  }

  #[cfg(test)]
  mod tests {
      // ... existing tests ...

      #[test]
      fn test_get_active_command() {
          use super::*;
          assert_eq!(get_active_command(ViewMode::Interface), "ifconfig");
          assert_eq!(get_active_command(ViewMode::Network), "ifconfig");
          assert_eq!(get_active_command(ViewMode::Connections), "netstat -an");
          assert_eq!(get_active_command(ViewMode::Ports), "lsof -iTCP -sTCP:LISTEN -P -n");
          assert_eq!(get_active_command(ViewMode::Routes), "netstat -rn");
          assert_eq!(get_active_command(ViewMode::Timeline), "event-logger");
      }
  }
  ```

- [ ] **Step 2: Run cargo test to verify tests compile and pass**
  Run: `cargo test`
  Expected: PASS (all tests including `test_get_active_command` pass).

- [ ] **Step 3: Commit changes**
  ```bash
  git add src/ui.rs
  git commit -m "feat: add get_active_command mapper and tests"
  ```

---

### Task 2: Layout & Render Pipeline Update

**Files:**
- Modify: [src/ui.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/ui.rs)

- [ ] **Step 1: Update vertical layout Constraints and indices in `ui.rs::draw`**
  Modify constraints from 4 to 5:
  ```rust
      let chunks = Layout::default()
          .direction(Direction::Vertical)
          .constraints([
              Constraint::Min(3),                    // 0: Top pane
              Constraint::Length(1),                 // 1: Active Command Panel (NEW)
              Constraint::Length(5),                 // 2: Recent Events Panel
              Constraint::Length(filter_bar_height), // 3: Filter Bar
              Constraint::Length(1),                 // 4: Status Bar
          ])
          .split(frame.size());
  ```
  And update references to Event Panel, Filter Bar, and Status Bar indices:
  ```rust
      // Event panel on chunks[2]
      frame.render_widget(event_list, chunks[2]);

      // Filter bar on chunks[3]
      if filter_bar_height > 0 {
          ...
          frame.render_widget(filter_p, chunks[3]);
      }

      // Status Bar on chunks[4]
      let status_idx = 4;
      ...
      frame.render_widget(status_p, chunks[status_idx]);
  ```

- [ ] **Step 2: Render Active Command Line on `chunks[1]`**
  Draw the single line paragraph in terminal style inside `ui.rs::draw`:
  ```rust
      // 2. Active Command Panel
      let command_str = get_active_command(app.view_mode);
      let command_line = Line::from(vec![
          Span::styled("$ ", Style::default().fg(Color::Rgb(0, 255, 102)).add_modifier(Modifier::BOLD)),
          Span::styled(command_str, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
      ]);
      let command_p = Paragraph::new(command_line);
      frame.render_widget(command_p, chunks[1]);
  ```

- [ ] **Step 3: Run tests and check for no panic**
  Run: `cargo test`
  Expected: PASS

- [ ] **Step 4: Commit and finalize**
  ```bash
  git commit -am "feat: implement active command panel UI and layout split"
  ```

---

## Verification Plan

### Automated Tests
- `cargo test` (ensures `test_get_active_command` passes and UI rendering does not panic).

### Manual Verification
- Run `cargo run`.
- Verify the active command panel is visible right above the "Recent Events" header.
- Switch views (`i`, `n`, `c`, `p`, `e`, `g`) and verify that the command string updates instantly according to the active mode.
