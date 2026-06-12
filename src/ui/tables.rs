use super::*;

pub(super) fn render_ports_table(frame: &mut Frame, app: &App, block: Block<'_>, area: Rect) {
    let header = Row::new([
        Cell::from(port_header_label(app, PortSortColumn::Proto, "Proto")),
        Cell::from(port_header_label(app, PortSortColumn::Port, "Port")),
        Cell::from(port_header_label(app, PortSortColumn::Command, "Command")),
        Cell::from(port_header_label(app, PortSortColumn::Pid, "PID")),
        Cell::from(port_header_label(app, PortSortColumn::User, "User")),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = app
        .navigation_items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let NavigationItem::ListeningPort {
                proto,
                port,
                command,
                pid,
                user,
                ..
            } = item
            else {
                return None;
            };

            let style = if idx == app.selected_index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Some(
                Row::new([
                    highlighted_filter_cell(proto.to_uppercase(), &app.port_filter),
                    highlighted_filter_cell(port.clone(), &app.port_filter),
                    highlighted_filter_cell(command.clone(), &app.port_filter),
                    highlighted_filter_cell(pid.clone(), &app.port_filter),
                    highlighted_filter_cell(user.clone(), &app.port_filter),
                ])
                .style(style),
            )
        })
        .collect();

    let rows = visible_rows(rows, app.selected_index, visible_table_rows(area.height));

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Length(6),
            Constraint::Min(8),
            Constraint::Length(7),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .column_spacing(1)
    .block(block);

    frame.render_widget(table, area);
}

pub(super) fn render_connections_table(frame: &mut Frame, app: &App, block: Block<'_>, area: Rect) {
    let header = Row::new([
        Cell::from(connection_header_label(
            app,
            ConnectionSortColumn::Proto,
            "Proto",
        )),
        Cell::from(connection_header_label(
            app,
            ConnectionSortColumn::Local,
            "Local IP",
        )),
        Cell::from("LPort"),
        Cell::from(connection_header_label(
            app,
            ConnectionSortColumn::Foreign,
            "Foreign IP",
        )),
        Cell::from("FPort"),
        Cell::from(connection_header_label(
            app,
            ConnectionSortColumn::State,
            "State",
        )),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = app
        .navigation_items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let NavigationItem::Connection {
                proto,
                local_ip,
                local_port,
                foreign_ip,
                foreign_port,
                state,
                ..
            } = item
            else {
                return None;
            };

            let style = if idx == app.selected_index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Some(
                Row::new([
                    highlighted_filter_cell(proto.to_uppercase(), &app.connection_filter),
                    highlighted_filter_cell(
                        app.profile_ip_display(local_ip),
                        &app.connection_filter,
                    ),
                    highlighted_filter_cell(local_port.clone(), &app.connection_filter),
                    highlighted_filter_cell(
                        app.profile_ip_display(foreign_ip),
                        &app.connection_filter,
                    ),
                    highlighted_filter_cell(foreign_port.clone(), &app.connection_filter),
                    highlighted_filter_cell(
                        state.clone().unwrap_or_default(),
                        &app.connection_filter,
                    ),
                ])
                .style(style),
            )
        })
        .collect();

    let rows = visible_rows(rows, app.selected_index, visible_table_rows(area.height));

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Length(10),
            Constraint::Length(5),
            Constraint::Length(10),
            Constraint::Length(5),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .column_spacing(1)
    .block(block);

    frame.render_widget(table, area);
}

fn port_header_label(app: &App, column: PortSortColumn, label: &str) -> String {
    if app.port_sort_column != column {
        return label.to_string();
    }

    let arrow = match app.port_sort_direction {
        SortDirection::Ascending => "↑",
        SortDirection::Descending => "↓",
    };
    format!("{label} {arrow}")
}

fn connection_header_label(app: &App, column: ConnectionSortColumn, label: &str) -> String {
    if app.connection_sort_column != column {
        return label.to_string();
    }

    let arrow = match app.connection_sort_direction {
        SortDirection::Ascending => "↑",
        SortDirection::Descending => "↓",
    };
    format!("{label} {arrow}")
}

fn route_header_label(app: &App, column: RouteSortColumn, label: &str) -> String {
    if app.route_inspector.sort_column != column {
        return label.to_string();
    }

    let arrow = match app.route_inspector.route_sort_direction {
        SortDirection::Ascending => "↑",
        SortDirection::Descending => "↓",
    };
    format!("{label} {arrow}")
}

fn visible_table_rows(area_height: u16) -> usize {
    area_height.saturating_sub(3).max(1).into()
}

fn visible_rows<T>(mut rows: Vec<T>, selected_index: usize, max_visible: usize) -> Vec<T> {
    if rows.is_empty() || max_visible == 0 {
        return Vec::new();
    }

    let max_visible = max_visible.min(rows.len());
    let selected_index = selected_index.min(rows.len() - 1);
    let mut start = selected_index.saturating_sub(max_visible.saturating_sub(1));
    let last_possible_start = rows.len() - max_visible;
    if start > last_possible_start {
        start = last_possible_start;
    }

    rows.drain(0..start);
    rows.truncate(max_visible);
    rows
}

pub(super) fn render_routes_table(frame: &mut Frame, app: &App, block: Block<'_>, area: Rect) {
    let header = Row::new([
        Cell::from(route_header_label(
            app,
            RouteSortColumn::Destination,
            "Destination",
        )),
        Cell::from(route_header_label(app, RouteSortColumn::Gateway, "Gateway")),
        Cell::from(route_header_label(
            app,
            RouteSortColumn::Interface,
            "Interface",
        )),
        Cell::from(route_header_label(app, RouteSortColumn::Metric, "Metric")),
        Cell::from("Protocol"),
        Cell::from("Flags"),
        Cell::from("Family"),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = app
        .filtered_sorted_routes()
        .into_iter()
        .enumerate()
        .map(|(idx, (_, route))| {
            let metric = route
                .metric
                .map(|metric| metric.to_string())
                .unwrap_or_else(|| "-".to_string());

            let style = if idx == app.selected_index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if is_default_route(route) {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else if is_vpn_interface_name(&route.interface) {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            Row::new([
                highlighted_filter_cell(
                    route.destination.clone(),
                    &app.route_inspector.route_filter,
                ),
                highlighted_filter_cell(route.gateway.clone(), &app.route_inspector.route_filter),
                highlighted_filter_cell(route.interface.clone(), &app.route_inspector.route_filter),
                Cell::from(metric),
                Cell::from(route.protocol.as_deref().unwrap_or("-")),
                Cell::from(route.flags.as_deref().unwrap_or("-")),
                Cell::from(route_family_label(route.family)),
            ])
            .style(style)
        })
        .collect();

    let rows = visible_rows(rows, app.selected_index, visible_table_rows(area.height));

    let table = Table::new(
        rows,
        [
            Constraint::Length(18),
            Constraint::Length(16),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Length(7),
        ],
    )
    .header(header)
    .column_spacing(1)
    .block(block);

    frame.render_widget(table, area);
}

pub(super) fn format_endpoint(ip: &str, port: &str) -> String {
    format!("{ip}:{port}")
}

fn highlighted_filter_cell(value: String, query: &str) -> Cell<'static> {
    let query = query.trim();
    if query.is_empty() {
        return Cell::from(value);
    }

    let value_lower = value.to_lowercase();
    let query_lower = query.to_lowercase();
    let mut spans = Vec::new();
    let mut cursor = 0;

    while let Some(offset) = value_lower[cursor..].find(&query_lower) {
        let start = cursor + offset;
        let end = start + query_lower.len();
        if !value.is_char_boundary(start) || !value.is_char_boundary(end) {
            break;
        }

        if start > cursor {
            spans.push(Span::raw(value[cursor..start].to_string()));
        }
        spans.push(Span::styled(
            value[start..end].to_string(),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        cursor = end;
    }

    if spans.is_empty() {
        Cell::from(value)
    } else {
        if cursor < value.len() {
            spans.push(Span::raw(value[cursor..].to_string()));
        }
        Cell::from(Line::from(spans))
    }
}
