use std::{fs, process::Command, sync::OnceLock};

use crate::app::{
    App, ConnectionDetailsSection, ConnectionSortColumn, NavigationItem, PortDetailsSection,
    PortSortColumn, SortDirection, ViewMode,
};
use crate::model::{
    InterfaceStatus, NetworkKind, ProcessMetrics, RouteDiagnosticSeverity, RouteFamily,
    RouteInspectorSection, RouteSortColumn, Subnet,
};
use crate::route_inspector::diagnostics::is_default_route;
use crate::route_inspector::graph::{build_route_graph, render_route_graph_lines};
use crate::route_inspector::vpn::is_vpn_interface_name;
use chrono::{DateTime, Local};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Sparkline, Table, Wrap,
    },
    Frame,
};

mod details;
mod overlays;
mod tables;
mod tools;

use details::{
    calculate_ipv4_subnet_u32, calculate_ipv6_subnet_arr, connection_details_section_tabs,
    connection_summary_lines, connection_whois_lines, format_bps, port_details_section_tabs,
    port_process_lines, port_summary_lines, prefix_len_to_ipv4_mask,
    render_route_inspector_details, resolve_connection_interface, route_family_label,
};
use overlays::{
    build_command_panel, command_panel_height, draw_help, draw_profile_editor,
    draw_profile_switcher, draw_raw_viewer, draw_release_notes_viewer,
};
use tables::{format_endpoint, render_connections_table, render_ports_table, render_routes_table};
use tools::{render_tools_input_modal, render_tools_view};

pub fn render_title() -> &'static str {
    "lazyifconfig"
}

fn header_line(app: &App) -> Line<'static> {
    let release_label = format!(
        " v{} ({})",
        env!("CARGO_PKG_VERSION"),
        release_date_label(app.latest_release_date.as_deref())
    );

    let mut spans = vec![
        Span::styled(
            "🦥 Lazyifconfig",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(release_label, Style::default().fg(Color::Yellow)),
        Span::styled(" - ", Style::default().fg(Color::DarkGray)),
        Span::styled(os_display_label(), Style::default().fg(Color::White)),
    ];

    spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
    spans.push(Span::styled(
        app.profile_status_text(),
        Style::default().fg(if app.profile_warning.is_some() {
            Color::Yellow
        } else {
            Color::LightCyan
        }),
    ));

    if let Some(metrics) = app.process_metrics.as_ref() {
        if let Some(summary) = format_process_metrics(metrics) {
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(summary, Style::default().fg(Color::LightCyan)));
        }
    }

    Line::from(spans)
}

fn release_date_label(release_date: Option<&str>) -> String {
    release_date
        .and_then(|value| value.split('T').next())
        .filter(|value| !value.is_empty())
        .map(std::string::ToString::to_string)
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_process_metrics(metrics: &ProcessMetrics) -> Option<String> {
    let mut parts = Vec::new();

    if let Some(cpu_tenths) = metrics.cpu_usage_tenths {
        parts.push(format!("CPU {}%", format_tenths(cpu_tenths)));
    }

    if let Some(rss) = metrics.memory_rss_bytes {
        parts.push(format!("MEM {}", format_bytes(rss)));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" | "))
    }
}

fn format_tenths(value: u16) -> String {
    format!("{}.{:01}", value / 10, value % 10)
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;

    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1}GB", bytes as f64 / GIB)
    } else if bytes >= 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / MIB)
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / KIB)
    } else {
        format!("{bytes}B")
    }
}

fn os_display_label() -> &'static str {
    static OS_LABEL: OnceLock<String> = OnceLock::new();
    OS_LABEL.get_or_init(detect_os_label).as_str()
}

fn detect_os_label() -> String {
    if cfg!(target_os = "macos") {
        let version = Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout).ok()
                } else {
                    None
                }
            })
            .map(|version| version.trim().to_string())
            .filter(|version| !version.is_empty());

        if let Some(version) = version {
            format!("macOS {version}")
        } else {
            "macOS".to_string()
        }
    } else if cfg!(target_os = "linux") {
        linux_os_label().unwrap_or_else(|| "Linux".to_string())
    } else {
        std::env::consts::OS.to_string()
    }
}

fn linux_os_label() -> Option<String> {
    let os_release = fs::read_to_string("/etc/os-release").ok()?;
    let pretty_name = os_release_value(&os_release, "PRETTY_NAME");
    let version = os_release_value(&os_release, "VERSION_ID");

    pretty_name
        .or_else(|| version.map(|version| format!("Linux {version}")))
        .filter(|label| !label.is_empty())
}

fn os_release_value(contents: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    contents.lines().find_map(|line| {
        let value = line.strip_prefix(&prefix)?;
        Some(value.trim_matches('"').to_string())
    })
}

fn get_active_command(view_mode: ViewMode) -> &'static str {
    match view_mode {
        ViewMode::Interface | ViewMode::Network => {
            if cfg!(target_os = "linux") {
                "ip -details -statistics address show"
            } else if cfg!(target_os = "windows") {
                "ipconfig /all"
            } else {
                "ifconfig"
            }
        }
        ViewMode::Connections => "netstat -an",
        ViewMode::Ports => {
            if cfg!(target_os = "linux") {
                "ss -H -ltnp"
            } else if cfg!(target_os = "windows") {
                "netstat -ano -p tcp"
            } else {
                "lsof +c 0 -iTCP -sTCP:LISTEN -P -n"
            }
        }
        ViewMode::Routes => {
            if cfg!(target_os = "linux") {
                "ip route show"
            } else if cfg!(target_os = "windows") {
                "route PRINT"
            } else {
                "netstat -rn"
            }
        }
        ViewMode::Tools => "tool-runner",
        ViewMode::Timeline => "event-logger",
    }
}

fn get_status_text(app: &App) -> String {
    match app.view_mode {
        ViewMode::Connections => {
            if app.connection_filter_active {
                " filter: type | Enter apply | Esc clear | Backspace delete ".to_string()
            } else {
                format!(
                    " q | / filter | s sort | S dir | Tab | c copy | w whois | {} | [/] ",
                    connection_sort_label(app)
                )
            }
        }
        ViewMode::Ports => {
            if app.port_filter_active {
                " filter: type | Enter apply | Esc clear | Backspace delete ".to_string()
            } else {
                format!(
                    " q | r | / filter | s sort | S dir | Tab | K kill | {} | [/] | i/n/c/e/g ",
                    port_sort_label(app)
                )
            }
        }
        ViewMode::Timeline => {
            " q | u check | U update | R notes | S save | [/] | i/n/c/p/g | j/k ".to_string()
        }
        ViewMode::Routes => {
            if app.route_inspector.route_filter_active {
                " filter routes: type | Enter apply | Esc clear | Backspace delete ".to_string()
            } else if app.route_inspector.destination_input_active {
                " destination: type | Enter lookup | Esc cancel | Backspace delete ".to_string()
            } else {
                format!(
                    " q | u check | U update | R notes | Enter path | Tab section | Home/End/1-4/5 | / filter | s sort | S dir | o raw | sort:{} | i/n/c/p/e ",
                    route_sort_label(app)
                )
            }
        }
        ViewMode::Tools => {
            if app.tools.input_modal_open {
                " input modal | type | Backspace | Tab field | Enter run | Esc cancel ".to_string()
            } else if app.tools.selected_tool_is_dns_lookup() {
                " q | P profiles | Enter input | / input | r rerun | o raw | [/] scroll | i/n/p/c/g/e "
                    .to_string()
            } else {
                " q | P profiles | Enter input | / input | r rerun | [/] scroll | i/n/p/c/g/e "
                    .to_string()
            }
        }
        _ => {
            format!(
                " q | P profiles | r | u check | U update | R notes | a:{} | i/n/c/p/e/g ",
                if app.show_all { "on" } else { "off" }
            )
        }
    }
}

fn port_sort_label(app: &App) -> String {
    format!(
        "{} {}",
        match app.port_sort_column {
            PortSortColumn::Port => "Port",
            PortSortColumn::Command => "Command",
            PortSortColumn::Pid => "PID",
            PortSortColumn::User => "User",
            PortSortColumn::Proto => "Proto",
        },
        match app.port_sort_direction {
            SortDirection::Ascending => "asc",
            SortDirection::Descending => "desc",
        }
    )
}

fn connection_sort_label(app: &App) -> String {
    format!(
        "{} {}",
        match app.connection_sort_column {
            ConnectionSortColumn::Local => "Local",
            ConnectionSortColumn::Foreign => "Foreign",
            ConnectionSortColumn::State => "State",
            ConnectionSortColumn::Proto => "Proto",
        },
        match app.connection_sort_direction {
            SortDirection::Ascending => "asc",
            SortDirection::Descending => "desc",
        }
    )
}

fn route_sort_label(app: &App) -> String {
    format!(
        "{} {}",
        match app.route_inspector.sort_column {
            RouteSortColumn::Destination => "Destination",
            RouteSortColumn::Gateway => "Gateway",
            RouteSortColumn::Interface => "Interface",
            RouteSortColumn::Metric => "Metric",
        },
        match app.route_inspector.route_sort_direction {
            SortDirection::Ascending => "asc",
            SortDirection::Descending => "desc",
        }
    )
}

fn view_tabs(view_mode: ViewMode) -> Line<'static> {
    let tabs = [
        (ViewMode::Interface, "Interface(i)"),
        (ViewMode::Network, "Network(n)"),
        (ViewMode::Ports, "Port(p)"),
        (ViewMode::Connections, "Connection(c)"),
        (ViewMode::Routes, "Route(g)"),
        (ViewMode::Tools, "Tools(t)"),
        (ViewMode::Timeline, "Timeline(e)"),
    ];

    let mut spans = Vec::new();
    for (idx, (mode, label)) in tabs.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        }

        let style = if *mode == view_mode {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        spans.push(Span::styled(format!(" {label} "), style));
    }

    Line::from(spans)
}

pub fn draw(frame: &mut Frame, app: &App) {
    let filter_bar_height: u16 = if app.port_filter_active
        || app.connection_filter_active
        || app.route_inspector.route_filter_active
        || (app.view_mode == ViewMode::Ports && !app.port_filter.is_empty())
        || (app.view_mode == ViewMode::Connections && !app.connection_filter.is_empty())
        || (app.view_mode == ViewMode::Routes && !app.route_inspector.route_filter.is_empty())
    {
        1
    } else {
        0
    };
    let command_panel_height = command_panel_height(app);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),                    // 0: App Header
            Constraint::Length(1),                    // 1: View Tabs
            Constraint::Min(3),                       // 2: Top pane
            Constraint::Length(command_panel_height), // 3: Active Command Panel
            Constraint::Length(5),                    // 4: Recent Events Panel
            Constraint::Length(filter_bar_height),    // 5: Filter Bar
            Constraint::Length(1),                    // 6: Status Bar
        ])
        .split(frame.size());

    let header =
        Paragraph::new(header_line(app)).style(Style::default().bg(Color::Rgb(24, 24, 24)));
    frame.render_widget(header, chunks[0]);

    let tabs =
        Paragraph::new(view_tabs(app.view_mode)).style(Style::default().bg(Color::Rgb(32, 32, 32)));
    frame.render_widget(tabs, chunks[1]);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(match app.view_mode {
            ViewMode::Routes => [Constraint::Percentage(70), Constraint::Percentage(30)],
            _ => [Constraint::Percentage(40), Constraint::Percentage(60)],
        })
        .split(top_chunks_area(chunks[2]));

    // Helper to extract size safely (compatible with older/newer ratatui area/size)
    fn top_chunks_area(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
        area
    }

    // 1. Left Pane: Interfaces or Subnets list
    let title = match app.view_mode {
        ViewMode::Interface => " Interfaces ",
        ViewMode::Network => " Networks (Subnet View) ",
        ViewMode::Connections => " Active Connections ",
        ViewMode::Ports => " Listening Ports ",
        ViewMode::Timeline => " Event Timeline ",
        ViewMode::Routes => " Routes ",
        ViewMode::Tools => " Tools ",
    };
    let list_block = Block::default().borders(Borders::ALL).title(title);

    let mut list_items = Vec::new();
    for (idx, item) in app.navigation_items.iter().enumerate() {
        let style = if idx == app.selected_index {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        match item {
            NavigationItem::SubnetHeader(subnet) => {
                let text = match subnet {
                    Subnet::Ipv4 {
                        network,
                        prefix_len,
                    } => format!("▼ {}/{}", network, prefix_len),
                    Subnet::Ipv6 {
                        network,
                        prefix_len,
                    } => format!("▼ {}/{}", network, prefix_len),
                    Subnet::Unassigned => "▼ Unassigned / No IP".to_string(),
                };
                let header_style = if idx == app.selected_index {
                    style
                } else {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                };
                list_items.push(ListItem::new(text).style(header_style));
            }
            NavigationItem::Interface {
                name,
                associated_ip,
            } => {
                let mut status_indicator = "○";
                let mut is_up = false;
                let mut kind = NetworkKind::Unknown;

                if let Some(snapshot) = &app.current_snapshot {
                    if let Some(interface) = snapshot.interfaces.iter().find(|i| i.name == *name) {
                        is_up = interface.status == InterfaceStatus::Up;
                        status_indicator = if is_up { "●" } else { "○" };
                        kind = interface.network_kind;
                    }
                }

                let mut display_text = if app.view_mode == ViewMode::Network {
                    format!(
                        "  {} {} ({})",
                        status_indicator,
                        name,
                        associated_ip.as_deref().unwrap_or("no IP")
                    )
                } else {
                    format!(
                        "{} {} ({})",
                        status_indicator,
                        name,
                        associated_ip.as_deref().unwrap_or("no IP")
                    )
                };

                // Add padding to display classification right-aligned nicely
                let padding = 35_usize.saturating_sub(display_text.chars().count());
                display_text.push_str(&" ".repeat(padding));
                display_text.push_str(kind.as_str());

                let mut final_style = style;
                if !is_up {
                    if idx == app.selected_index {
                        final_style = final_style.add_modifier(Modifier::DIM);
                    } else {
                        final_style = final_style.fg(Color::DarkGray);
                    }
                }
                list_items.push(ListItem::new(display_text).style(final_style));
            }
            NavigationItem::Connection {
                proto,
                local_ip,
                local_port,
                foreign_ip,
                foreign_port,
                state,
                ..
            } => {
                let state_str = state
                    .as_ref()
                    .map(|s| format!(" ({})", s))
                    .unwrap_or_default();
                let text = format!(
                    "[{}] {} -> {}{}",
                    proto.to_uppercase(),
                    format_endpoint(local_ip, local_port),
                    format_endpoint(foreign_ip, foreign_port),
                    state_str
                );
                list_items.push(ListItem::new(text).style(style));
            }
            NavigationItem::ListeningPort {
                proto,
                port,
                command,
                pid,
                ..
            } => {
                let text = format!(
                    "[{}] :{:<6} {} (PID: {})",
                    proto.to_uppercase(),
                    port,
                    command,
                    pid
                );
                list_items.push(ListItem::new(text).style(style));
            }
            NavigationItem::Event {
                index,
                kind,
                timestamp,
                message,
            } => {
                let datetime: DateTime<Local> = (*timestamp).into();
                let time_str = datetime.format("%H:%M:%S").to_string();
                let text = format!("{} [{}] {}", time_str, kind.as_str(), message);

                // Color code based on severity
                let mut item_style = style;
                if idx != app.selected_index {
                    if let Some(event) = app.recent_events.get(*index) {
                        match event.severity {
                            crate::model::EventSeverity::Warning => {
                                item_style = item_style.fg(Color::Yellow)
                            }
                            crate::model::EventSeverity::Error => {
                                item_style = item_style.fg(Color::Red)
                            }
                            crate::model::EventSeverity::Info => {}
                        }
                    }
                }
                list_items.push(ListItem::new(text).style(item_style));
            }
            NavigationItem::Route {
                destination,
                gateway,
                interface,
                index,
            } => {
                let text = format!("{:<18} {:<16} {}", destination, gateway, interface);
                let route_style = app
                    .current_snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.routes.get(*index))
                    .map(|route| {
                        if is_default_route(route) {
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD)
                        } else if is_vpn_interface_name(&route.interface) {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default()
                        }
                    })
                    .unwrap_or_default();
                let final_style = if idx == app.selected_index {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    route_style
                };
                list_items.push(ListItem::new(text).style(final_style));
            }
        }
    }
    if app.view_mode == ViewMode::Tools {
        render_tools_view(frame, app, top_chunks[0], top_chunks[1]);
    } else if app.view_mode == ViewMode::Ports {
        render_ports_table(frame, app, list_block, top_chunks[0]);
    } else if app.view_mode == ViewMode::Connections {
        render_connections_table(frame, app, list_block, top_chunks[0]);
    } else if app.view_mode == ViewMode::Routes {
        render_routes_table(frame, app, list_block, top_chunks[0]);
    } else {
        let list_widget = List::new(list_items).block(list_block);
        frame.render_widget(list_widget, top_chunks[0]);
    }

    // 2. Right Pane: Details Panel
    if app.view_mode != ViewMode::Tools {
        let details_block = Block::default().borders(Borders::ALL).title(" Details ");

        let details_inner = details_block.inner(top_chunks[1]);
        frame.render_widget(details_block, top_chunks[1]);

        if app.view_mode == ViewMode::Routes {
            render_route_inspector_details(frame, app, details_inner);
        } else if let Some(selected_item) = app.navigation_items.get(app.selected_index) {
            match selected_item {
                NavigationItem::SubnetHeader(subnet) => {
                    let mut details_text = String::new();
                    details_text.push_str("=== Subnet Information ===\n\n");
                    match subnet {
                        Subnet::Ipv4 {
                            network,
                            prefix_len,
                        } => {
                            details_text.push_str("Protocol:       IPv4\n");
                            details_text.push_str(&format!("Network Addr:   {}\n", network));
                            details_text.push_str(&format!("Prefix Length:  {}\n", prefix_len));
                            details_text.push_str(&format!(
                                "Subnet Mask:    {}\n",
                                prefix_len_to_ipv4_mask(*prefix_len)
                            ));
                        }
                        Subnet::Ipv6 {
                            network,
                            prefix_len,
                        } => {
                            details_text.push_str("Protocol:       IPv6\n");
                            details_text.push_str(&format!("Network Addr:   {}\n", network));
                            details_text.push_str(&format!("Prefix Length:  {}\n", prefix_len));
                        }
                        Subnet::Unassigned => {
                            details_text.push_str("Protocol:       N/A\n");
                            details_text.push_str(
                                "Description:    Interfaces without an IP Address assigned.\n",
                            );
                        }
                    }

                    details_text.push_str("\nMember Interfaces:\n");
                    if let Some(snapshot) = &app.current_snapshot {
                        for interface in &snapshot.interfaces {
                            let mut matches_subnet = false;
                            let mut ip_val = "no IP".to_string();

                            match subnet {
                                Subnet::Ipv4 {
                                    network,
                                    prefix_len,
                                } => {
                                    for addr in &interface.ipv4 {
                                        if let Some(p) = addr.prefix_len {
                                            if p == *prefix_len {
                                                if let Ok(ip) =
                                                    addr.value.parse::<std::net::Ipv4Addr>()
                                                {
                                                    let net_ip =
                                                        calculate_ipv4_subnet_u32(u32::from(ip), p);
                                                    if net_ip == *network {
                                                        matches_subnet = true;
                                                        ip_val = addr.value.clone();
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Subnet::Ipv6 {
                                    network,
                                    prefix_len,
                                } => {
                                    for addr in &interface.ipv6 {
                                        if let Some(p) = addr.prefix_len {
                                            if p == *prefix_len {
                                                if let Ok(ip) =
                                                    addr.value.parse::<std::net::Ipv6Addr>()
                                                {
                                                    let net_ip = calculate_ipv6_subnet_arr(&ip, p);
                                                    if net_ip == *network {
                                                        matches_subnet = true;
                                                        ip_val = addr.value.clone();
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Subnet::Unassigned => {
                                    let has_ipv4 =
                                        interface.ipv4.iter().any(|a| a.prefix_len.is_some());
                                    let has_ipv6 =
                                        interface.ipv6.iter().any(|a| a.prefix_len.is_some());
                                    if !has_ipv4 && !has_ipv6 {
                                        matches_subnet = true;
                                    }
                                }
                            }

                            if matches_subnet {
                                details_text
                                    .push_str(&format!("  - {} ({})\n", interface.name, ip_val));
                            }
                        }
                    }

                    let details_p = Paragraph::new(details_text)
                        .wrap(Wrap { trim: true })
                        .scroll((app.details_scroll, 0));
                    frame.render_widget(details_p, details_inner);
                }
                NavigationItem::Interface { name, .. } => {
                    if let Some(snapshot) = &app.current_snapshot {
                        if let Some(interface) =
                            snapshot.interfaces.iter().find(|i| i.name == *name)
                        {
                            let sub_chunks = Layout::default()
                                .direction(Direction::Vertical)
                                .constraints([Constraint::Min(5), Constraint::Length(6)])
                                .split(details_inner);

                            let mut details_text = String::new();
                            details_text.push_str(&format!("Name:           {}\n", interface.name));
                            details_text.push_str(&format!(
                                "Classification: {}\n",
                                interface.network_kind.as_str()
                            ));
                            details_text.push_str(&format!(
                                "Status:         {}\n",
                                match interface.status {
                                    InterfaceStatus::Up => "Active / Up",
                                    InterfaceStatus::Down => "Inactive / Down",
                                }
                            ));
                            details_text.push_str(&format!(
                                "MAC Address:    {}\n",
                                interface.mac_address.as_deref().unwrap_or("N/A")
                            ));
                            details_text.push_str(&format!(
                                "MTU:            {}\n",
                                interface
                                    .mtu
                                    .map(|m| m.to_string())
                                    .unwrap_or_else(|| "N/A".to_string())
                            ));

                            details_text.push_str("\nIPv4 Addresses:\n");
                            for addr in &interface.ipv4 {
                                let gw_str = addr
                                    .gateway
                                    .as_ref()
                                    .map(|g| format!(" (Gateway: {})", g))
                                    .unwrap_or_default();
                                details_text.push_str(&format!(
                                    "  - {} / {}{}\n",
                                    addr.value,
                                    addr.prefix_len
                                        .map(|p| p.to_string())
                                        .unwrap_or_else(|| "?".to_string()),
                                    gw_str
                                ));
                            }
                            details_text.push_str("IPv6 Addresses:\n");
                            for addr in &interface.ipv6 {
                                let gw_str = addr
                                    .gateway
                                    .as_ref()
                                    .map(|g| format!(" (Gateway: {})", g))
                                    .unwrap_or_default();
                                details_text.push_str(&format!(
                                    "  - {} / {}{}\n",
                                    addr.value,
                                    addr.prefix_len
                                        .map(|p| p.to_string())
                                        .unwrap_or_else(|| "?".to_string()),
                                    gw_str
                                ));
                            }

                            details_text.push_str("\nTraffic Cumulative Stats:\n");
                            if let Some(stats) = &interface.stats {
                                details_text.push_str(&format!(
                                    "  Packets: RX {} / TX {}\n",
                                    stats.rx_packets, stats.tx_packets
                                ));
                                details_text.push_str(&format!(
                                    "  Bytes:   RX {} / TX {}\n",
                                    stats.rx_bytes, stats.tx_bytes
                                ));
                            } else {
                                details_text.push_str("  No stats available\n");
                            }

                            let details_p = Paragraph::new(details_text)
                                .wrap(Wrap { trim: true })
                                .scroll((app.details_scroll, 0));
                            frame.render_widget(details_p, sub_chunks[0]);

                            // Render Charts
                            let chart_chunks = Layout::default()
                                .direction(Direction::Horizontal)
                                .constraints([
                                    Constraint::Percentage(50),
                                    Constraint::Percentage(50),
                                ])
                                .split(sub_chunks[1]);

                            let (rx_rate, tx_rate) = app.selected_rates().unwrap_or((0, 0));

                            let mut rx_data = vec![0u64; 40];
                            let mut tx_data = vec![0u64; 40];
                            if let Some(history) = app.traffic_history.get(&interface.name) {
                                let rx_len = history.rx_rates.len();
                                let rx_start = 40_usize.saturating_sub(rx_len);
                                for (i, &val) in history.rx_rates.iter().enumerate() {
                                    if rx_start + i < 40 {
                                        rx_data[rx_start + i] = val;
                                    }
                                }
                                let tx_len = history.tx_rates.len();
                                let tx_start = 40_usize.saturating_sub(tx_len);
                                for (i, &val) in history.tx_rates.iter().enumerate() {
                                    if tx_start + i < 40 {
                                        tx_data[tx_start + i] = val;
                                    }
                                }
                            }

                            let rx_title = format!(" RX Rate: {} ", format_bps(rx_rate));
                            let tx_title = format!(" TX Rate: {} ", format_bps(tx_rate));

                            let rx_sparkline = Sparkline::default()
                                .block(Block::default().borders(Borders::ALL).title(rx_title))
                                .style(Style::default().fg(Color::Green))
                                .data(&rx_data);

                            let tx_sparkline = Sparkline::default()
                                .block(Block::default().borders(Borders::ALL).title(tx_title))
                                .style(Style::default().fg(Color::Yellow))
                                .data(&tx_data);

                            frame.render_widget(rx_sparkline, chart_chunks[0]);
                            frame.render_widget(tx_sparkline, chart_chunks[1]);
                        }
                    }
                }
                NavigationItem::Connection {
                    proto,
                    local_ip,
                    local_port,
                    foreign_ip,
                    foreign_port,
                    state,
                    index: _,
                } => {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(1), Constraint::Min(0)])
                        .split(details_inner);

                    let tab_line = Paragraph::new(connection_details_section_tabs(app))
                        .style(Style::default().fg(Color::White).bg(Color::Rgb(24, 24, 24)));
                    frame.render_widget(tab_line, chunks[0]);

                    let lines = match app.connection_details_section {
                        ConnectionDetailsSection::Summary => {
                            connection_summary_lines(details::ConnectionSummaryInput {
                                proto,
                                local_ip_display: app.profile_ip_display(local_ip),
                                local_port,
                                foreign_ip_display: app.profile_ip_display(foreign_ip),
                                foreign_ip_raw: foreign_ip,
                                foreign_port,
                                state: state.as_deref(),
                                mapped_interface: resolve_connection_interface(app, local_ip),
                            })
                        }
                        ConnectionDetailsSection::Whois => connection_whois_lines(app, foreign_ip),
                    };

                    let details_p = Paragraph::new(lines)
                        .wrap(Wrap { trim: true })
                        .scroll((app.details_scroll, 0));
                    frame.render_widget(details_p, chunks[1]);
                }
                NavigationItem::ListeningPort {
                    proto,
                    port,
                    command,
                    pid,
                    user,
                    ..
                } => {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(1), Constraint::Min(0)])
                        .split(details_inner);

                    let tab_line = Paragraph::new(port_details_section_tabs(app))
                        .style(Style::default().fg(Color::White).bg(Color::Rgb(24, 24, 24)));
                    frame.render_widget(tab_line, chunks[0]);

                    let lines = match app.port_details_section {
                        PortDetailsSection::Summary => port_summary_lines(proto, port, pid, user),
                        PortDetailsSection::Process => port_process_lines(command, pid, user),
                    };

                    let details_p = Paragraph::new(lines)
                        .wrap(Wrap { trim: true })
                        .scroll((app.details_scroll, 0));
                    frame.render_widget(details_p, chunks[1]);
                }
                NavigationItem::Event {
                    index,
                    kind,
                    timestamp,
                    message,
                } => {
                    let mut lines = Vec::new();
                    lines.push(Line::from(Span::styled(
                        "=== Event Details ===",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled(
                            "Type:        ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            kind.as_str(),
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));

                    let datetime: DateTime<Local> = (*timestamp).into();
                    let time_str = datetime.format("%Y-%m-%d %H:%M:%S").to_string();
                    lines.push(Line::from(vec![
                        Span::styled(
                            "Time:        ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(time_str),
                    ]));

                    let severity_str = if let Some(event) = app.recent_events.get(*index) {
                        event.severity.as_str()
                    } else {
                        "INFO"
                    };
                    let severity_color = match severity_str {
                        "WARNING" => Color::Yellow,
                        "ERROR" => Color::Red,
                        _ => Color::Green,
                    };
                    lines.push(Line::from(vec![
                        Span::styled(
                            "Severity:    ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            severity_str,
                            Style::default()
                                .fg(severity_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));

                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled(
                            "Description: ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(message.as_str()),
                    ]));

                    let impact = match kind {
                    crate::model::NetworkEventKind::VpnConnected => "Traffic may be routed through VPN. Default routes might change.",
                    crate::model::NetworkEventKind::VpnDisconnected => "VPN connection lost. Traffic will not be routed through VPN.",
                    crate::model::NetworkEventKind::ContainerNetworkAppeared => "Local container networking active. Container services might be reachable.",
                    crate::model::NetworkEventKind::ContainerNetworkRemoved => "Local container networking inactive.",
                    crate::model::NetworkEventKind::InterfaceAppeared => "New network interface is discovered and registered.",
                    crate::model::NetworkEventKind::InterfaceRemoved => "Interface has been removed or disabled.",
                    crate::model::NetworkEventKind::InterfaceUp => "Network interface is now active and up.",
                    crate::model::NetworkEventKind::InterfaceDown => "Network interface is inactive. No traffic can flow.",
                    crate::model::NetworkEventKind::Ipv4Added | crate::model::NetworkEventKind::Ipv6Added => "IP address assigned. Communications on this subnet are now enabled.",
                    crate::model::NetworkEventKind::Ipv4Removed | crate::model::NetworkEventKind::Ipv6Removed => "IP address unassigned. Host loses addressability on this subnet.",
                    crate::model::NetworkEventKind::Ipv4Changed | crate::model::NetworkEventKind::Ipv6Changed => "IP address has changed. Active sockets on this interface might drop.",
                    crate::model::NetworkEventKind::ProcessKilled => "The process holding the listening port has been terminated. Port is now free.",
                    crate::model::NetworkEventKind::ActionCopied => "An IP address has been successfully copied to your system clipboard.",
                    crate::model::NetworkEventKind::ActionWhois => "WHOIS query initiated to fetch metadata for the foreign IP address.",
                    crate::model::NetworkEventKind::SystemError => "A command or system level call returned an error status.",
                    crate::model::NetworkEventKind::PublicIpChanged => "Your public IP address has changed. Network route or VPN activation might have occurred.",
                    crate::model::NetworkEventKind::ProviderChanged => "Your ISP or network provider has changed. Active routing paths updated.",
                    crate::model::NetworkEventKind::UpdateAvailable => "A newer GitHub release was found and is ready to install.",
                    crate::model::NetworkEventKind::UpdateInstalled => "A new binary has been installed. Restart the app to run the updated version.",
                    crate::model::NetworkEventKind::UpdateCheckFailed => "The GitHub release check or install step failed.",
                    crate::model::NetworkEventKind::TimelineExported => "Timeline was saved to disk for offline review.",
                };
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        "=== Expected Impact ===",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::raw(impact)));

                    let details_p = Paragraph::new(lines)
                        .wrap(Wrap { trim: true })
                        .scroll((app.details_scroll, 0));
                    frame.render_widget(details_p, details_inner);
                }
                NavigationItem::Route { .. } => {}
            }
        } else {
            let details_p = Paragraph::new("No data collected yet. Press 'r' to refresh.")
                .wrap(Wrap { trim: true })
                .scroll((app.details_scroll, 0));
            frame.render_widget(details_p, details_inner);
        }
    }

    // 3. Active Command Panel
    let (command_lines, command_style) = build_command_panel(app);
    let command_p = Paragraph::new(command_lines)
        .style(command_style)
        .wrap(Wrap { trim: true });
    frame.render_widget(command_p, chunks[3]);

    // 4. Event Panel
    let event_block = Block::default()
        .borders(Borders::ALL)
        .title(" Recent Events ");
    let mut event_items = Vec::new();
    for event in app.recent_events.iter().rev().take(10) {
        let datetime: DateTime<Local> = event.timestamp.into();
        let time_str = datetime.format("%H:%M:%S").to_string();

        let mut item_style = Style::default();
        match event.severity {
            crate::model::EventSeverity::Warning => item_style = item_style.fg(Color::Yellow),
            crate::model::EventSeverity::Error => item_style = item_style.fg(Color::Red),
            _ => {}
        }

        event_items
            .push(ListItem::new(format!("[{}] {}", time_str, event.message)).style(item_style));
    }
    let event_list = List::new(event_items).block(event_block);
    frame.render_widget(event_list, chunks[4]);

    // 5. Filter Bar
    if filter_bar_height > 0 {
        let (filter_value, filter_active) = if app.view_mode == ViewMode::Connections {
            (app.connection_filter.as_str(), app.connection_filter_active)
        } else if app.view_mode == ViewMode::Routes {
            (
                app.route_inspector.route_filter.as_str(),
                app.route_inspector.route_filter_active,
            )
        } else {
            (app.port_filter.as_str(), app.port_filter_active)
        };
        let filter_text = if filter_active {
            format!(" 🔍 Filter: {}▌", filter_value)
        } else {
            format!(" 🔍 Filter: {}  (/: edit, Esc: clear)", filter_value)
        };
        let filter_style = if filter_active {
            Style::default().bg(Color::DarkGray).fg(Color::Yellow)
        } else {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        };
        let filter_p = Paragraph::new(filter_text).style(filter_style);
        frame.render_widget(filter_p, chunks[5]);
    }

    // 6. Status Bar
    let status_idx = 6;
    let status_text = get_status_text(app);
    let status_p = Paragraph::new(status_text).style(
        Style::default()
            .bg(Color::Black)
            .fg(Color::LightYellow)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(status_p, chunks[status_idx]);

    if app.help_visible {
        draw_help(frame);
    }

    if app.release_notes_viewer.active {
        draw_release_notes_viewer(frame, app);
    }

    if app.profile_switcher.active {
        draw_profile_switcher(frame, app);
    }

    if app.profile_editor.active {
        draw_profile_editor(frame, app);
    }

    if app.raw_viewer.active {
        draw_raw_viewer(frame, app);
    }

    if app.view_mode == ViewMode::Tools && app.tools.input_modal_open {
        render_tools_input_modal(frame, app);
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::model::{
        NetworkSnapshot, ProcessMetrics, RouteDiagnostic, RouteDiagnosticSeverity, RouteEntry,
        RouteFamily, RouteInspectorSection, RoutePathResult,
    };
    use crate::ui::details::{route_diagnostic_lines, route_inspector_section_tabs};
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn test_ui_draw_no_panic() {
        let app = App::default();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut has_borders = false;
        for cell in buffer.content() {
            if cell.symbol() == "│" || cell.symbol() == "─" {
                has_borders = true;
                break;
            }
        }
        assert!(has_borders);
    }

    #[test]
    fn test_ui_draw_network_view_no_panic() {
        let mut app = App::default();
        app.view_mode = ViewMode::Network;
        app.navigation_items = vec![
            NavigationItem::SubnetHeader(crate::model::Subnet::Unassigned),
            NavigationItem::Interface {
                name: "en0".to_string(),
                associated_ip: Some("192.168.0.15".to_string()),
            },
        ];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
    }

    #[test]
    fn test_ui_draw_timeline_view_no_panic() {
        let mut app = App::default();
        app.view_mode = ViewMode::Timeline;
        app.recent_events.push(crate::model::NetworkEvent::new(
            crate::model::NetworkEventKind::VpnConnected,
            crate::model::EventSeverity::Info,
            "utun0 connected".to_string(),
        ));
        app.update_navigation_items();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
    }

    fn route_test_app(section: RouteInspectorSection) -> App {
        let mut app = App::default();
        app.view_mode = ViewMode::Routes;
        app.route_inspector.active_section = section;
        app.current_snapshot = Some(NetworkSnapshot {
            routes: vec![
                route_entry("default", "192.168.0.1", "en0", RouteFamily::Ipv4),
                route_entry("10.8.0.0/24", "10.8.0.1", "utun4", RouteFamily::Ipv4),
            ],
            ..NetworkSnapshot::default()
        });
        app.update_navigation_items();
        app
    }

    fn route_entry(
        destination: &str,
        gateway: &str,
        interface: &str,
        family: RouteFamily,
    ) -> RouteEntry {
        RouteEntry {
            destination: destination.to_string(),
            gateway: gateway.to_string(),
            interface: interface.to_string(),
            metric: Some(100),
            protocol: Some("static".to_string()),
            flags: Some("UGSc".to_string()),
            family,
        }
    }

    fn draw_to_string(app: &mut App) -> String {
        let backend = TestBackend::new(120, 32);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    #[test]
    fn test_ui_draw_routes_summary_section() {
        let mut app = route_test_app(RouteInspectorSection::Summary);

        let rendered = draw_to_string(&mut app);

        assert!(rendered.contains("Route Summary"));
        assert!(rendered.contains("192.168.0.1"));
        assert!(rendered.contains("VPN"));
    }

    #[test]
    fn test_route_detail_tabs_show_all_sections() {
        let app = route_test_app(RouteInspectorSection::Summary);

        let tabs = route_inspector_section_tabs(&app);
        let mut text = String::new();
        for span in &tabs.spans {
            text.push_str(span.content.as_ref());
        }

        assert!(text.contains("Summary"));
        assert!(text.contains("Path"));
        assert!(text.contains("VPN"));
        assert!(text.contains("Diagnostics"));
        assert!(text.contains("1:Summary"));

        let summary_tab = tabs.spans.iter().find(|span| span.content == " 1:Summary ");
        assert!(summary_tab.is_some());
    }

    #[test]
    fn test_ui_draw_routes_path_viewer_with_result() {
        let mut app = route_test_app(RouteInspectorSection::PathViewer);
        app.route_inspector.latest_path_result = Some(RoutePathResult {
            destination: "8.8.8.8".to_string(),
            resolved_destination: Some("8.8.8.8".to_string()),
            source_ip: Some("192.168.0.25".to_string()),
            interface: Some("en0".to_string()),
            gateway: Some("192.168.0.1".to_string()),
            is_vpn: false,
            raw_output: String::new(),
        });

        let rendered = draw_to_string(&mut app);

        assert!(rendered.contains("Path Viewer"));
        assert!(rendered.contains("This Host"));
        assert!(rendered.contains("8.8.8.8"));
    }

    #[test]
    fn test_ui_draw_routes_diagnostics_section() {
        let mut app = route_test_app(RouteInspectorSection::Diagnostics);
        app.route_inspector.diagnostics = vec![RouteDiagnostic {
            severity: RouteDiagnosticSeverity::Warning,
            title: "Route interface is down".to_string(),
            description: "A route points to an interface that is currently down.".to_string(),
            affected_route: Some(route_entry(
                "default",
                "192.168.0.1",
                "en0",
                RouteFamily::Ipv4,
            )),
            recommendation: "Bring the interface up or remove the stale route.".to_string(),
        }];

        let rendered = draw_to_string(&mut app);

        assert!(rendered.contains("Diagnostics"));
        assert!(rendered.contains("Route interface is down"));
        assert!(rendered.contains("Recommendation"));
    }

    #[test]
    fn test_ui_draw_routes_details_without_selected_route() {
        let mut app = App {
            view_mode: ViewMode::Routes,
            current_snapshot: Some(NetworkSnapshot::default()),
            ..Default::default()
        };
        app.route_inspector.active_section = RouteInspectorSection::Summary;
        app.update_navigation_items();

        let rendered = draw_to_string(&mut app);

        assert!(rendered.contains("Route Summary"));
        assert!(rendered.contains("No default route"));
        assert!(!rendered.contains("No data collected yet"));
    }

    #[test]
    fn test_route_sort_direction_changes_order() {
        let mut app = route_test_app(RouteInspectorSection::Summary);
        app.current_snapshot = Some(NetworkSnapshot {
            routes: vec![
                route_entry("10.0.0.0/8", "192.168.0.1", "en0", RouteFamily::Ipv4),
                route_entry("0.0.0.0/0", "10.0.0.2", "utun4", RouteFamily::Ipv4),
            ],
            ..NetworkSnapshot::default()
        });
        app.route_inspector.sort_column = RouteSortColumn::Metric;
        app.route_inspector.route_sort_direction = SortDirection::Descending;
        if let Some(snapshot) = app.current_snapshot.as_mut() {
            snapshot.routes[0].metric = Some(20);
            snapshot.routes[1].metric = Some(5);
        }

        let sorted = app
            .filtered_sorted_routes()
            .into_iter()
            .map(|(_, route)| route.destination.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            sorted,
            vec!["10.0.0.0/8".to_string(), "0.0.0.0/0".to_string()]
        );
    }

    #[test]
    fn test_route_diagnostics_color_all_diagnostic_components_by_severity() {
        let mut app = route_test_app(RouteInspectorSection::Diagnostics);
        app.route_inspector.diagnostics = vec![RouteDiagnostic {
            severity: RouteDiagnosticSeverity::Warning,
            title: "Route interface is down".to_string(),
            description: "A route points to an interface that is currently down.".to_string(),
            affected_route: Some(route_entry(
                "default",
                "192.168.0.1",
                "en0",
                RouteFamily::Ipv4,
            )),
            recommendation: "Bring the interface up or remove the stale route.".to_string(),
        }];

        let lines = route_diagnostic_lines(&app);

        assert_eq!(lines[2].spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(lines[3].spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(lines[3].spans[1].style.fg, Some(Color::Yellow));
        assert_eq!(lines[4].spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(lines[4].spans[1].style.fg, Some(Color::Yellow));
        assert_eq!(lines[5].spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(lines[5].spans[1].style.fg, Some(Color::Yellow));
    }

    #[test]
    fn test_get_active_command() {
        let interface_command = if cfg!(target_os = "linux") {
            "ip -details -statistics address show"
        } else if cfg!(target_os = "windows") {
            "ipconfig /all"
        } else {
            "ifconfig"
        };
        assert_eq!(get_active_command(ViewMode::Interface), interface_command);
        assert_eq!(get_active_command(ViewMode::Network), interface_command);
        assert_eq!(get_active_command(ViewMode::Connections), "netstat -an");
        let ports_command = if cfg!(target_os = "linux") {
            "ss -H -ltnp"
        } else if cfg!(target_os = "windows") {
            "netstat -ano -p tcp"
        } else {
            "lsof +c 0 -iTCP -sTCP:LISTEN -P -n"
        };
        assert_eq!(get_active_command(ViewMode::Ports), ports_command);
        let route_command = if cfg!(target_os = "linux") {
            "ip route show"
        } else if cfg!(target_os = "windows") {
            "route PRINT"
        } else {
            "netstat -rn"
        };
        assert_eq!(get_active_command(ViewMode::Routes), route_command);
        assert_eq!(get_active_command(ViewMode::Timeline), "event-logger");
    }

    #[test]
    fn test_command_line_shows_output_and_help_hints() {
        let app = App::default();
        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("o[output]"));
        assert!(rendered.contains("?[help]"));
    }

    #[test]
    fn test_bottom_status_bar_uses_high_contrast_style() {
        let app = App::default();
        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let cell = buffer.get(1, 23);

        assert_eq!(cell.bg, Color::Black);
        assert_eq!(cell.fg, Color::LightYellow);
        assert!(cell.modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_top_tabs_show_view_shortcuts() {
        let mut app = App::default();
        app.view_mode = ViewMode::Ports;

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("Interface(i)"));
        assert!(rendered.contains("Network(n)"));
        assert!(rendered.contains("Port(p)"));
        assert!(rendered.contains("Connection(c)"));
        assert!(rendered.contains("Route(g)"));
        assert!(rendered.contains("Timeline(e)"));
    }

    #[test]
    fn test_top_header_shows_app_name_and_os() {
        let app = App::default();
        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("🦥"));
        assert!(rendered.contains("Lazyifconfig"));
        assert!(rendered.contains(" - "));
        assert!(rendered.contains(os_display_label()));
    }

    #[test]
    fn test_top_header_shows_process_cpu_and_memory_usage() {
        let mut app = App::default();
        app.process_metrics = Some(ProcessMetrics {
            cpu_usage_tenths: Some(42),
            memory_rss_bytes: Some(128 * 1024 * 1024),
        });

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("CPU 4.2%"));
        assert!(rendered.contains("MEM 128.0MB"));
    }

    #[test]
    fn test_ports_view_renders_table_columns() {
        let mut app = App::default();
        app.view_mode = ViewMode::Ports;
        app.navigation_items = vec![NavigationItem::ListeningPort {
            proto: "tcp".to_string(),
            port: "8080".to_string(),
            command: "my-server".to_string(),
            pid: "12345".to_string(),
            user: "alice".to_string(),
            index: 0,
        }];

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let mut left_pane = String::new();
        for y in 0..24 {
            for x in 0..48 {
                left_pane.push_str(buffer.get(x, y).symbol());
            }
        }

        assert!(left_pane.contains("Proto"));
        assert!(left_pane.contains("Port ↑"));
        assert!(left_pane.contains("Command"));
        assert!(left_pane.contains("PID"));
        assert!(left_pane.contains("User"));
        assert!(left_pane.contains("TCP"));
        assert!(left_pane.contains("8080"));
        assert!(left_pane.contains("my-server"));
        assert!(left_pane.contains("12345"));
        assert!(left_pane.contains("alice"));
    }

    #[test]
    fn test_ports_filter_highlights_matching_text() {
        let mut app = App::default();
        app.view_mode = ViewMode::Ports;
        app.port_filter = "server".to_string();
        app.navigation_items = vec![NavigationItem::ListeningPort {
            proto: "tcp".to_string(),
            port: "8080".to_string(),
            command: "my-server".to_string(),
            pid: "12345".to_string(),
            user: "alice".to_string(),
            index: 0,
        }];

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let has_highlight = (2..18).any(|y| {
            (0..48).any(|x| {
                let cell = buffer.get(x, y);
                cell.bg == Color::Yellow
                    && cell.fg == Color::Black
                    && cell.modifier.contains(Modifier::BOLD)
            })
        });

        assert!(has_highlight);
    }

    #[test]
    fn test_connections_view_renders_table_columns() {
        let mut app = App::default();
        app.view_mode = ViewMode::Connections;
        app.navigation_items = vec![NavigationItem::Connection {
            proto: "tcp".to_string(),
            local_ip: "127.0.0.1".to_string(),
            local_port: "5".to_string(),
            foreign_ip: "1.1.1.1".to_string(),
            foreign_port: "443".to_string(),
            state: Some("ESTAB".to_string()),
            index: 0,
        }];

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let mut left_pane = String::new();
        for y in 0..24 {
            for x in 0..48 {
                left_pane.push_str(buffer.get(x, y).symbol());
            }
        }

        assert!(left_pane.contains("Proto"));
        assert!(left_pane.contains("Local IP ↑"));
        assert!(left_pane.contains("LPort"));
        assert!(left_pane.contains("Foreign IP"));
        assert!(left_pane.contains("FPort"));
        assert!(left_pane.contains("State"));
        assert!(left_pane.contains("TCP"));
        assert!(left_pane.contains("127.0.0.1"));
        assert!(left_pane.contains("5"));
        assert!(left_pane.contains("1.1.1.1"));
        assert!(left_pane.contains("443"));
        assert!(left_pane.contains("ESTAB"));
    }

    #[test]
    fn test_connections_filter_highlights_matching_text() {
        let mut app = App::default();
        app.view_mode = ViewMode::Connections;
        app.connection_filter = "1.1.1.1".to_string();
        app.navigation_items = vec![NavigationItem::Connection {
            proto: "tcp".to_string(),
            local_ip: "127.0.0.1".to_string(),
            local_port: "5".to_string(),
            foreign_ip: "1.1.1.1".to_string(),
            foreign_port: "443".to_string(),
            state: Some("ESTAB".to_string()),
            index: 0,
        }];

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let has_highlight = (2..18).any(|y| {
            (0..48).any(|x| {
                let cell = buffer.get(x, y);
                cell.bg == Color::Yellow
                    && cell.fg == Color::Black
                    && cell.modifier.contains(Modifier::BOLD)
            })
        });

        assert!(has_highlight);
    }

    #[test]
    fn test_routes_view_shows_route_rows() {
        let app = route_test_app(RouteInspectorSection::Summary);
        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let mut left_pane = String::new();
        for y in 0..24 {
            for x in 0..48 {
                left_pane.push_str(buffer.get(x, y).symbol());
            }
        }

        assert!(left_pane.contains("default"));
        assert!(left_pane.contains("192.168.0.1"));
        assert!(left_pane.contains("utun4"));
    }

    #[test]
    fn test_routes_view_renders_route_table_headers() {
        let app = route_test_app(RouteInspectorSection::Summary);
        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..24 {
            for x in 0..120 {
                rendered.push_str(buffer.get(x, y).symbol());
            }
        }

        assert!(rendered.contains("Destination"));
        assert!(rendered.contains("Gateway"));
        assert!(rendered.contains("Interface"));
        assert!(rendered.contains("Metric"));
        assert!(rendered.contains("Protocol"));
        assert!(rendered.contains("Flags"));
        assert!(rendered.contains("Family"));
    }

    #[test]
    fn test_routes_filter_applies_to_route_list() {
        let mut app = route_test_app(RouteInspectorSection::Summary);
        app.route_inspector.route_filter = "utun4".to_string();
        app.update_navigation_items();

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let mut left_pane = String::new();
        for y in 0..24 {
            for x in 0..48 {
                left_pane.push_str(buffer.get(x, y).symbol());
            }
        }

        assert!(left_pane.contains("utun4"));
        assert!(!left_pane.contains("192.168.0.1"));
    }

    #[test]
    fn test_routes_view_highlights_selected_route_row() {
        let mut app = route_test_app(RouteInspectorSection::Summary);
        app.selected_index = 1;

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let has_selected = (2..20).any(|y| {
            (0..48).any(|x| {
                let cell = buffer.get(x, y);
                cell.bg == Color::Yellow && cell.fg == Color::Black
            })
        });

        assert!(has_selected);
    }

    #[test]
    fn draw_tools_view_lists_all_tool_entries() {
        let mut app = App::default();
        app.set_view_mode(ViewMode::Tools);

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("Tools(t)"));
        assert!(rendered.contains("DNS Lookup"));
        assert!(rendered.contains("Port Check"));
        assert!(rendered.contains("Ping"));
        assert!(rendered.contains("Whois Lookup"));
        assert!(rendered.contains("IP Information"));
        assert!(rendered.contains("TLS Inspector"));
        assert!(rendered.contains("Traceroute"));
        assert!(rendered.contains("Input"));
        assert!(rendered.contains("Results"));
        assert!(rendered.contains("Raw Output"));
    }

    #[test]
    fn draw_tools_view_shows_input_modal_when_open() {
        let mut app = App::default();
        app.set_view_mode(ViewMode::Tools);
        app.tools.open_input_modal();

        assert_eq!(app.tools.selected_field_index, 0);

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("Tool Input"));
        assert!(rendered.contains("Target"));
        assert!(rendered.contains("Enter run"));
        assert!(rendered.contains("Esc cancel"));

        let buffer = terminal.backend().buffer();
        let has_focus_highlight = (0..30).any(|y| {
            (0..120).any(|x| {
                let cell = buffer.get(x, y);
                cell.bg == Color::Yellow && cell.fg == Color::Black
            })
        });

        assert!(has_focus_highlight);
        assert!(!rendered.contains("8.8.8.8"));
    }

    #[test]
    fn draw_tools_input_modal_mutes_placeholders_and_shows_warning() {
        let mut app = App::default();
        app.set_view_mode(ViewMode::Tools);
        app.tools.select_next_tool();
        app.tools.select_next_tool();
        app.tools.select_next_tool();
        app.tools.open_input_modal();

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..30 {
            for x in 0..120 {
                rendered.push_str(buffer.get(x, y).symbol());
            }
        }

        assert!(rendered.contains("Warning: fix the input issues before running."));

        assert!(rendered.contains("443"));
    }

    #[test]
    fn test_update_available_renders_loud_banner() {
        let mut app = App::default();
        app.update_status = crate::update::UpdateStatus::Available {
            version: "9.9.9".to_string(),
        };
        app.pending_update = Some(crate::update::AvailableUpdate {
            current_version: "0.1.0".to_string(),
            target_version: "9.9.9".to_string(),
            release_url: "https://example.com/release".to_string(),
            asset_name: "lazyifconfig-v9.9.9-aarch64-apple-darwin.tar.gz".to_string(),
            download_url: "https://example.com/release.tar.gz".to_string(),
            release_date: "2026-01-01T12:34:56Z".to_string(),
            release_notes: "Big networking refresh\nFaster route parsing\nExtra diagnostics"
                .to_string(),
        });

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("UPDATE READY"));
        assert!(rendered.contains("PRESS U TO INSTALL"));
        assert!(rendered.contains("v9.9.9"));
        assert!(rendered.contains("Big networking refresh"));
    }

    #[test]
    fn test_status_text_is_compact_without_raw_output_hint() {
        let modes = [
            ViewMode::Interface,
            ViewMode::Network,
            ViewMode::Connections,
            ViewMode::Ports,
            ViewMode::Timeline,
            ViewMode::Routes,
        ];

        for mode in modes {
            let mut app = App::default();
            app.view_mode = mode;
            let status = get_status_text(&app);

            assert!(!status.contains("Raw Output"));
            let max_len = if mode == ViewMode::Routes { 170 } else { 90 };
            assert!(
                status.len() <= max_len,
                "status too long for {:?}: {}",
                mode,
                status
            );
        }
    }

    #[test]
    fn test_status_text_mentions_update_actions() {
        let app = App::default();
        let status = get_status_text(&app);

        assert!(status.contains("u check"));
        assert!(status.contains("U update"));
        assert!(status.contains("R notes"));
    }

    #[test]
    fn test_help_mentions_update_shortcuts() {
        let mut app = App::default();
        app.help_visible = true;

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("u check updates"));
        assert!(rendered.contains("U apply update"));
        assert!(rendered.contains("R release notes"));
    }

    #[test]
    fn test_release_notes_popup_renders_full_body() {
        let mut app = App::default();
        app.release_notes_viewer.active = true;
        app.pending_update = Some(crate::update::AvailableUpdate {
            current_version: "0.1.0".to_string(),
            target_version: "9.9.9".to_string(),
            release_url: "https://example.com/release".to_string(),
            asset_name: "lazyifconfig-v9.9.9-aarch64-apple-darwin.tar.gz".to_string(),
            download_url: "https://example.com/release.tar.gz".to_string(),
            release_date: "2026-01-01T12:34:56Z".to_string(),
            release_notes: "## Highlights\n- Faster scans\n- Better update UI\n- Route fixes"
                .to_string(),
        });

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("Release Notes"));
        assert!(rendered.contains("v9.9.9"));
        assert!(rendered.contains("Faster scans"));
        assert!(rendered.contains("Better update UI"));
    }
}
