use super::*;

pub(super) fn draw_help(frame: &mut Frame) {
    let area = get_centered_rect(54, 42, frame.size());
    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::White));
    let inner = block.inner(area);

    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from("q quit    r refresh    a all interfaces"),
        Line::from("i interface  n network  c connections"),
        Line::from("p ports      e timeline g routes"),
        Line::from("u check updates   U apply update"),
        Line::from("R release notes    Esc close popup"),
        Line::from("o raw output ? help     s sort S save"),
        Line::from("j/k up/down  h/l tabs   [/]: details scroll"),
    ];
    let help = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .style(Style::default().bg(Color::Black).fg(Color::White));
    frame.render_widget(help, inner);
}

fn update_status_label(app: &App) -> String {
    match &app.update_status {
        crate::update::UpdateStatus::Idle => format!("v{}", env!("CARGO_PKG_VERSION")),
        crate::update::UpdateStatus::Checking { manual } => {
            if *manual {
                "update: checking(now)".to_string()
            } else {
                "update: checking".to_string()
            }
        }
        crate::update::UpdateStatus::Available { version } => {
            format!("update: v{version} ready")
        }
        crate::update::UpdateStatus::Installing { version, .. } => {
            format!("update: installing v{version}")
        }
        crate::update::UpdateStatus::Updated { version } => {
            format!("update: v{version} installed")
        }
        crate::update::UpdateStatus::UpToDate => "update: latest".to_string(),
        crate::update::UpdateStatus::Error { .. } => "update: error".to_string(),
    }
}

pub(super) fn command_panel_height(app: &App) -> u16 {
    if matches!(
        &app.update_status,
        crate::update::UpdateStatus::Available { .. }
    ) && app
        .pending_update
        .as_ref()
        .is_some_and(|update| !update.release_notes.trim().is_empty())
    {
        2
    } else {
        1
    }
}

pub(super) fn build_command_panel(app: &App) -> (Vec<Line<'static>>, Style) {
    let command_str = get_active_command(app.view_mode);
    match &app.update_status {
        crate::update::UpdateStatus::Available { version } => {
            let mut lines = vec![Line::from(vec![
                Span::styled(
                    " UPDATE READY ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("v{version}"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    "PRESS U TO INSTALL",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD | Modifier::RAPID_BLINK),
                ),
                Span::raw("   "),
                Span::styled(
                    "u re-check",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ])];

            if let Some(update) = &app.pending_update {
                let notes = summarize_release_notes_for_banner(&update.release_notes, 96);
                lines.push(Line::from(vec![
                    Span::styled(
                        " Notes: ",
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::LightYellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(notes, Style::default().fg(Color::White)),
                ]));
            }

            (lines, Style::default().bg(Color::DarkGray))
        }
        crate::update::UpdateStatus::Installing { version, .. } => (
            vec![Line::from(vec![
                Span::styled(
                    " INSTALLING UPDATE ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::LightBlue)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("v{version}"),
                    Style::default()
                        .fg(Color::LightBlue)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    "please wait",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ])],
            Style::default().bg(Color::DarkGray),
        ),
        crate::update::UpdateStatus::Updated { version } => (
            vec![Line::from(vec![
                Span::styled(
                    " UPDATE INSTALLED ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("v{version}"),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    "restart app",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ])],
            Style::default().bg(Color::DarkGray),
        ),
        crate::update::UpdateStatus::Error { .. } => (
            vec![Line::from(vec![
                Span::styled(
                    " UPDATE FAILED ",
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    "press u to check again",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            ])],
            Style::default().bg(Color::DarkGray),
        ),
        _ => (
            vec![Line::from(vec![
                Span::styled(
                    "$ ",
                    Style::default()
                        .fg(Color::Rgb(0, 255, 102))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    command_str,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("   "),
                Span::styled(
                    update_status_label(app),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("   "),
                Span::styled(
                    "o[output]",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    "?[help]",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ])],
            Style::default(),
        ),
    }
}

fn truncate_release_notes(notes: &str, max_chars: usize) -> String {
    let mut out = String::new();

    for ch in notes.chars() {
        if out.chars().count() >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }

    out
}

fn summarize_release_notes_for_banner(notes: &str, max_chars: usize) -> String {
    let mut summary_parts = Vec::new();

    for raw_line in notes.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let cleaned = trimmed
            .trim_start_matches('#')
            .trim_start_matches('-')
            .trim_start_matches('*')
            .trim_start_matches(' ')
            .trim();

        if cleaned.is_empty() {
            continue;
        }

        summary_parts.push(cleaned.to_string());
        if summary_parts.len() == 2 {
            break;
        }
    }

    let summary = if summary_parts.is_empty() {
        notes.trim().to_string()
    } else {
        summary_parts.join(" | ")
    };

    truncate_release_notes(&summary, max_chars)
}

pub(super) fn draw_release_notes_viewer(frame: &mut Frame, app: &App) {
    let area = if frame.size().width < 90 || frame.size().height < 28 {
        frame.size()
    } else {
        get_centered_rect(82, 78, frame.size())
    };

    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Release Notes ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black).fg(Color::White));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(update) = &app.pending_update else {
        let empty = Paragraph::new("No pending release notes. Press 'u' to check for updates.")
            .style(Style::default().bg(Color::Black).fg(Color::White));
        frame.render_widget(empty, inner);
        return;
    };

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(inner);

    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                "Version: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("v{}", update.target_version),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "Release: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(update.release_url.as_str()),
        ]),
    ])
    .style(Style::default().bg(Color::Black).fg(Color::White))
    .wrap(Wrap { trim: true });
    frame.render_widget(header, vertical[0]);

    let notes = Paragraph::new(update.release_notes.clone())
        .style(Style::default().bg(Color::Black).fg(Color::White))
        .wrap(Wrap { trim: false })
        .scroll((app.release_notes_viewer.scroll, 0));
    frame.render_widget(notes, vertical[1]);

    let footer = Paragraph::new("Esc/q/R close | j/k or arrows scroll")
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(footer, vertical[2]);
}

pub(super) fn draw_profile_switcher(frame: &mut Frame, app: &App) {
    let area = get_centered_rect(48, 48, frame.size());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Profiles ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::LightCyan))
        .style(Style::default().bg(Color::Black).fg(Color::White));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(inner);

    let items = app
        .available_profile_names
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            let active = name == app.active_profile_name();
            let marker = if active { "*" } else { " " };
            let style = if idx == app.profile_switcher.selected_index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if active {
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("{marker} {name}")).style(style)
        })
        .collect::<Vec<_>>();

    frame.render_widget(List::new(items), chunks[0]);

    let footer = Paragraph::new("Enter change | n add | e edit | Esc close")
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(footer, chunks[1]);
}

pub(super) fn draw_profile_editor(frame: &mut Frame, app: &App) {
    let area = get_centered_rect(62, 48, frame.size());
    frame.render_widget(Clear, area);

    let title = match app.profile_editor.mode {
        crate::app::ProfileEditorMode::New => " Add Profile ",
        crate::app::ProfileEditorMode::Edit => " Edit Profile ",
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::LightCyan))
        .style(Style::default().bg(Color::Black).fg(Color::White));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .split(inner);

    let field_style = |index| {
        if app.profile_editor.selected_field_index == index {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        }
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Name",
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("{}▌", app.profile_editor.name),
                field_style(0),
            )),
        ]),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Description",
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("{}▌", app.profile_editor.description),
                field_style(1),
            )),
        ]),
        chunks[1],
    );

    let counts = format!(
        "Networks: {} | Hosts: {} | Targets: {}",
        app.profile_editor.networks.len(),
        app.profile_editor.hosts.len(),
        app.profile_editor.targets.len()
    );
    frame.render_widget(
        Paragraph::new(counts).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );

    let candidate_items = app
        .profile_editor
        .detected_candidates
        .iter()
        .enumerate()
        .map(|(idx, candidate)| {
            let style = if app.profile_editor.selected_field_index == 2
                && idx == app.profile_editor.selected_candidate_index
            {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(candidate.label.clone()).style(style)
        })
        .collect::<Vec<_>>();

    let candidates = if candidate_items.is_empty() {
        List::new(vec![
            ListItem::new("No detected candidates").style(Style::default().fg(Color::DarkGray))
        ])
    } else {
        List::new(candidate_items)
    }
    .block(
        Block::default()
            .title(" Detected candidates ")
            .borders(Borders::ALL)
            .border_style(if app.profile_editor.selected_field_index == 2 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            }),
    );
    frame.render_widget(candidates, chunks[3]);

    if let Some(message) = &app.profile_editor.message {
        frame.render_widget(
            Paragraph::new(message.clone()).style(Style::default().fg(Color::Yellow)),
            chunks[4],
        );
    }

    let footer =
        Paragraph::new("Tab field | candidates: j/k select, a add | Enter save | Esc close")
            .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(footer, chunks[5]);
}

fn get_centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn build_matched_line<'a>(
    line: &'a str,
    matches_in_line: &[&crate::app::SearchMatch],
    text_color: Color,
    highlight_color: Color,
) -> Line<'a> {
    let mut spans = Vec::new();
    let mut last_idx = 0;

    for m in matches_in_line {
        if line.is_char_boundary(m.start_byte) && line.is_char_boundary(m.end_byte) {
            if m.start_byte > last_idx && line.is_char_boundary(last_idx) {
                spans.push(Span::styled(
                    &line[last_idx..m.start_byte],
                    Style::default().fg(text_color),
                ));
            }
            spans.push(Span::styled(
                &line[m.start_byte..m.end_byte],
                Style::default()
                    .fg(Color::Black)
                    .bg(highlight_color)
                    .add_modifier(Modifier::BOLD),
            ));
            last_idx = m.end_byte;
        }
    }

    if last_idx < line.len() && line.is_char_boundary(last_idx) {
        spans.push(Span::styled(
            &line[last_idx..],
            Style::default().fg(text_color),
        ));
    }

    Line::from(spans)
}

pub(super) fn draw_raw_viewer(frame: &mut Frame, app: &App) {
    let area = if frame.size().width < 80 || frame.size().height < 24 {
        frame.size()
    } else {
        get_centered_rect(80, 85, frame.size())
    };

    frame.render_widget(Clear, area);

    let main_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(68, 68, 68)))
        .style(Style::default().bg(Color::Rgb(0, 0, 0)));
    frame.render_widget(main_block, area);

    // Inner area for contents
    let inner_area = Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );

    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Tabs
            Constraint::Length(1), // Separator line
            Constraint::Min(3),    // Content
            Constraint::Length(1), // Status Bar / Search Bar
        ])
        .split(inner_area);

    // 1. Sources Tab Bar
    let mut tab_spans = vec![Span::styled(
        "Sources: ",
        Style::default().fg(Color::DarkGray),
    )];
    for (i, src) in app.raw_viewer.sources.iter().enumerate() {
        if i > 0 {
            tab_spans.push(Span::styled("  |  ", Style::default().fg(Color::DarkGray)));
        }
        let style = if i == app.raw_viewer.selected_index {
            Style::default()
                .bg(Color::Rgb(0, 255, 102))
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Rgb(192, 255, 192))
        };
        tab_spans.push(Span::styled(format!(" {} ", src.as_str()), style));
    }
    let tab_p =
        Paragraph::new(Line::from(tab_spans)).style(Style::default().bg(Color::Rgb(0, 0, 0)));
    frame.render_widget(tab_p, vertical_chunks[0]);

    // 2. Separator Line
    let separator_text = "─".repeat(inner_area.width as usize);
    let separator_p = Paragraph::new(separator_text).style(
        Style::default()
            .fg(Color::Rgb(68, 68, 68))
            .bg(Color::Rgb(0, 0, 0)),
    );
    frame.render_widget(separator_p, vertical_chunks[1]);

    // 3. Command Output Content
    let source_id = app.raw_viewer.sources.get(app.raw_viewer.selected_index);
    let mut lines = Vec::new();
    let mut text_store = String::new();

    if let Some(&src) = source_id {
        if let Some(output) = app.command_outputs.get(&src) {
            // Command prompt
            lines.push(Line::from(vec![
                Span::styled(
                    "$ ",
                    Style::default()
                        .fg(Color::Rgb(0, 255, 102))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    &output.command,
                    Style::default()
                        .fg(Color::Rgb(0, 255, 102))
                        .add_modifier(Modifier::BOLD),
                ),
            ]));

            // Timestamp and Exit Code
            let local_time: DateTime<Local> = output.executed_at.into();
            let time_str = local_time.format("%Y-%m-%d %H:%M:%S").to_string();
            let exit_str = match output.exit_code {
                Some(code) => code.to_string(),
                None => "None".to_string(),
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("Executed: {}  |  Exit Code: ", time_str),
                    Style::default().fg(Color::Rgb(128, 128, 128)),
                ),
                Span::styled(
                    exit_str,
                    Style::default().fg(if output.exit_code == Some(0) {
                        Color::Rgb(0, 255, 102)
                    } else {
                        Color::Rgb(255, 102, 102)
                    }),
                ),
            ]));
            lines.push(Line::raw(""));

            // Combined output text
            text_store.push_str(&output.stdout);
            text_store.push('\n');
            text_store.push_str(&output.stderr);
            let text_color = if output.exit_code == Some(0) {
                Color::Rgb(192, 255, 192)
            } else {
                Color::Rgb(255, 102, 102)
            };
            let highlight_color = Color::Rgb(255, 204, 0);

            for (line_idx, line) in text_store.lines().enumerate() {
                let matches_in_line: Vec<&crate::app::SearchMatch> = app
                    .raw_viewer
                    .search_matches
                    .iter()
                    .filter(|m| m.line_index == line_idx)
                    .collect();

                if matches_in_line.is_empty() {
                    lines.push(Line::styled(line, Style::default().fg(text_color)));
                } else {
                    lines.push(build_matched_line(
                        line,
                        &matches_in_line,
                        text_color,
                        highlight_color,
                    ));
                }
            }
        } else {
            lines.push(Line::styled(
                "Command execution history not found.",
                Style::default().fg(Color::Rgb(255, 102, 102)),
            ));
        }
    } else {
        lines.push(Line::styled(
            "No source selected.",
            Style::default().fg(Color::Rgb(255, 102, 102)),
        ));
    }

    let lines_count = lines.len();
    let content_height = vertical_chunks[2].height as usize;
    let max_scroll = lines_count.saturating_sub(content_height) as u16;
    let render_scroll = app.raw_viewer.scroll.min(max_scroll);

    let content_p = Paragraph::new(lines)
        .style(Style::default().bg(Color::Rgb(0, 0, 0)))
        .scroll((render_scroll, 0));
    frame.render_widget(content_p, vertical_chunks[2]);

    // 4. Status Bar / Search Prompt
    let status_line = if app.raw_viewer.search_active {
        Line::from(vec![
            Span::styled(
                "Search: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                &app.raw_viewer.search_query,
                Style::default().fg(Color::White),
            ),
            Span::styled("█", Style::default().fg(Color::Yellow)),
        ])
    } else if !app.raw_viewer.search_query.is_empty() {
        let current = if app.raw_viewer.search_matches.is_empty() {
            0
        } else {
            app.raw_viewer.current_match_index + 1
        };
        let total = app.raw_viewer.search_matches.len();
        Line::from(vec![
            Span::styled("Search: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!(
                    "{} ({} / {})  -  n: Next, N: Prev  |  ",
                    app.raw_viewer.search_query, current, total
                ),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                "Esc/q/o: Close | Tab: Next Src | y: Copy Cmd | Y: Copy Output",
                Style::default().fg(Color::Gray),
            ),
        ])
    } else {
        Line::from(Span::styled(
            "Esc/q/o: Close | Tab: Next Src | y: Copy Cmd | Y: Copy Output | /: Search",
            Style::default().fg(Color::Rgb(180, 180, 180)),
        ))
    };

    let status_p = Paragraph::new(status_line).style(Style::default().bg(Color::Rgb(30, 30, 30)));
    frame.render_widget(status_p, vertical_chunks[3]);
}
