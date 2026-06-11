use super::*;

pub(super) fn render_tools_view(frame: &mut Frame, app: &App, list_area: Rect, details_area: Rect) {
    let mut tool_items = Vec::new();
    for (idx, definition) in app.tools.registry.definitions().iter().enumerate() {
        let selected = idx == app.tools.selected_index;
        let mut style = if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let suffix = match definition.availability {
            crate::tools::ToolAvailability::Runnable => "",
            crate::tools::ToolAvailability::Planned => " (planned)",
        };
        if definition.availability == crate::tools::ToolAvailability::Planned && !selected {
            style = style.fg(Color::DarkGray);
        }
        tool_items.push(ListItem::new(format!("{}{}", definition.name, suffix)).style(style));
    }

    let tool_list =
        List::new(tool_items).block(Block::default().borders(Borders::ALL).title(" Tools "));
    frame.render_widget(tool_list, list_area);

    let definition = app.tools.selected_definition();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((definition.fields.len() as u16).saturating_add(4).max(5)),
            Constraint::Percentage(38),
            Constraint::Percentage(42),
        ])
        .split(details_area);

    let mut input_lines = Vec::new();
    input_lines.push(Line::from(Span::styled(
        definition.description,
        Style::default().fg(Color::White),
    )));
    input_lines.push(Line::from(""));

    let selected_input = app.tools.inputs.get(&definition.id);
    for (idx, field) in definition.fields.iter().enumerate() {
        let value = selected_input
            .and_then(|input| input.values.get(field.key))
            .map(String::as_str)
            .unwrap_or("");
        let is_focused = idx == app.tools.selected_field_index;
        let shown = if value.is_empty() && !is_focused {
            field.placeholder
        } else {
            value
        };
        let marker = if is_focused { ">" } else { " " };
        let field_style = if is_focused {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let shown_style = if value.is_empty() && !is_focused {
            Style::default().fg(Color::DarkGray)
        } else {
            field_style
        };
        input_lines.push(Line::from(vec![
            Span::styled(format!("{marker} {}: ", field.label), field_style),
            Span::styled(shown.to_string(), shown_style),
        ]));
    }

    let validation_errors = tools_input_validation_errors(app);
    if !validation_errors.is_empty() {
        input_lines.push(Line::from(""));
        input_lines.push(Line::from(Span::styled(
            "Warning: fix the input issues before running.",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        for error in &validation_errors {
            input_lines.push(Line::from(Span::styled(
                error.clone(),
                Style::default().fg(Color::LightYellow),
            )));
        }
    }

    if definition.availability == crate::tools::ToolAvailability::Planned {
        input_lines.push(Line::from(""));
        input_lines.push(Line::from(Span::styled(
            "planned / disabled",
            Style::default().fg(Color::DarkGray),
        )));
    }

    frame.render_widget(
        Paragraph::new(input_lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(" Input ")),
        chunks[0],
    );

    let state = app
        .tools
        .states
        .get(&definition.id)
        .copied()
        .unwrap_or(crate::tools::ToolExecutionState::Idle);
    let mut result_lines = Vec::new();
    match state {
        crate::tools::ToolExecutionState::Running => {
            result_lines.push(Line::from("Running..."));
        }
        crate::tools::ToolExecutionState::Failed => {
            result_lines.push(Line::from(Span::styled(
                "Error",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
            if let Some(error) = app.tools.errors.get(&definition.id) {
                result_lines.push(Line::from(error.clone()));
            }
        }
        _ => {
            if let Some(result) = app.tools.results.get(&definition.id) {
                for section in &result.sections {
                    result_lines.push(Line::from(Span::styled(
                        section.label.clone(),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )));
                    for line in &section.lines {
                        result_lines.push(Line::from(line.clone()));
                    }
                    result_lines.push(Line::from(""));
                }
            } else if definition.availability == crate::tools::ToolAvailability::Runnable {
                result_lines.push(Line::from("Press Enter to edit input, then Enter to run."));
            } else {
                result_lines.push(Line::from("This tool is planned for a follow-up slice."));
            }
        }
    }
    frame.render_widget(
        Paragraph::new(result_lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(" Results ")),
        chunks[1],
    );

    let raw_output = app
        .tools
        .results
        .get(&definition.id)
        .map(|result| result.raw_output.as_str())
        .unwrap_or("Raw output appears here after a tool runs.");
    let raw_output =
        if definition.id == crate::tools::ToolId::DnsLookup && !app.tools.dns_raw_output_expanded {
            "▶ Show Raw Output\nPress o to expand."
        } else {
            raw_output
        };

    let raw_lines: Vec<Line<'_>> = if raw_output.is_empty() {
        vec![Line::from("")]
    } else {
        let mut iter = raw_output.lines();
        let first = iter.next().unwrap_or("");
        let mut lines = Vec::new();
        let command_style = Style::default()
            .fg(Color::Rgb(0, 255, 102))
            .add_modifier(Modifier::BOLD);
        let output_style = Style::default().fg(Color::Rgb(192, 255, 192));
        if !first.is_empty() {
            if first.starts_with("$ ") {
                lines.push(Line::from(vec![
                    Span::styled("$ ", command_style),
                    Span::styled(&first[2..], command_style),
                ]));
            } else {
                lines.push(Line::styled(first, output_style));
            }
        }
        for line in iter {
            lines.push(Line::styled(line.to_string(), output_style));
        }
        lines
    };
    frame.render_widget(
        Paragraph::new(raw_lines)
            .wrap(Wrap { trim: false })
            .scroll((app.tools.raw_scroll, 0))
            .block(Block::default().borders(Borders::ALL).title(" Raw Output ")),
        chunks[2],
    );
}

pub(super) fn render_tools_input_modal(frame: &mut Frame, app: &App) {
    let definition = app.tools.selected_definition();
    if definition.fields.is_empty() {
        return;
    }

    let area = centered_rect(62, 46, frame.size());
    frame.render_widget(Clear, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let selected_input = app.tools.inputs.get(&definition.id);
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        definition.description,
        Style::default().fg(Color::White),
    )));
    lines.push(Line::from(""));

    for (idx, field) in definition.fields.iter().enumerate() {
        let value = selected_input
            .and_then(|input| input.values.get(field.key))
            .map(String::as_str)
            .unwrap_or("");
        let is_focused = idx == app.tools.selected_field_index;
        let shown = if value.is_empty() && !is_focused {
            field.placeholder
        } else {
            value
        };
        let marker = if is_focused { ">" } else { " " };
        let field_style = if is_focused {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let shown_style = if value.is_empty() && !is_focused {
            Style::default().fg(Color::DarkGray)
        } else {
            field_style
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} {}: ", field.label), field_style),
            Span::styled(shown.to_string(), shown_style),
        ]));
    }

    let validation_errors = tools_input_validation_errors(app);
    if !validation_errors.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Warning: fix the input issues before running.",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        for error in &validation_errors {
            lines.push(Line::from(Span::styled(
                error.clone(),
                Style::default().fg(Color::LightYellow),
            )));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(" Tool Input ")),
        area,
    );

    frame.render_widget(
        Paragraph::new("Enter run | Tab next field | Esc cancel")
            .style(Style::default().fg(Color::LightYellow).bg(Color::Black)),
        inner[1],
    );
}

fn tools_input_validation_errors(app: &App) -> Vec<String> {
    app.tools.selected_input_validation_errors()
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
