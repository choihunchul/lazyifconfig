use super::*;

pub(super) fn prefix_len_to_ipv4_mask(prefix_len: u8) -> String {
    let mask = if prefix_len == 0 {
        0
    } else if prefix_len >= 32 {
        u32::MAX
    } else {
        u32::MAX << (32 - prefix_len)
    };
    let octets = std::net::Ipv4Addr::from(mask).octets();
    format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3])
}

pub(super) fn calculate_ipv4_subnet_u32(ip_val: u32, prefix_len: u8) -> std::net::Ipv4Addr {
    let mask = if prefix_len == 0 {
        0
    } else if prefix_len >= 32 {
        u32::MAX
    } else {
        u32::MAX << (32 - prefix_len)
    };
    std::net::Ipv4Addr::from(ip_val & mask)
}

pub(super) fn calculate_ipv6_subnet_arr(
    ip: &std::net::Ipv6Addr,
    prefix_len: u8,
) -> std::net::Ipv6Addr {
    let octets = ip.octets();
    let mut mask_octets = [0u8; 16];
    for (i, mask_octet) in mask_octets.iter_mut().enumerate() {
        let bit_index = (i as u8) * 8;
        if prefix_len >= bit_index + 8 {
            *mask_octet = 0xff;
        } else if prefix_len <= bit_index {
            *mask_octet = 0x00;
        } else {
            let remaining = prefix_len - bit_index;
            *mask_octet = 0xff_u8.checked_shl((8 - remaining) as u32).unwrap_or(0);
        }
    }
    let mut subnet_octets = [0u8; 16];
    for (subnet_octet, (octet, mask_octet)) in subnet_octets
        .iter_mut()
        .zip(octets.iter().zip(mask_octets.iter()))
    {
        *subnet_octet = octet & mask_octet;
    }
    std::net::Ipv6Addr::from(subnet_octets)
}

pub(super) fn route_family_label(family: RouteFamily) -> &'static str {
    match family {
        RouteFamily::Ipv4 => "IPv4",
        RouteFamily::Ipv6 => "IPv6",
        RouteFamily::Unknown => "?",
    }
}

fn diagnostic_color(severity: RouteDiagnosticSeverity) -> Color {
    match severity {
        RouteDiagnosticSeverity::Info => Color::Blue,
        RouteDiagnosticSeverity::Warning => Color::Yellow,
        RouteDiagnosticSeverity::Error => Color::Red,
    }
}

pub(super) fn render_route_inspector_details(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    let tab_line = Paragraph::new(route_inspector_section_tabs(app))
        .style(Style::default().fg(Color::White).bg(Color::Rgb(24, 24, 24)));
    frame.render_widget(tab_line, chunks[0]);

    let lines = match app.route_inspector.active_section {
        RouteInspectorSection::Summary => route_summary_lines(app),
        RouteInspectorSection::PathViewer => route_path_lines(app),
        RouteInspectorSection::VpnRoutes => vpn_route_lines(app),
        RouteInspectorSection::Diagnostics => route_diagnostic_lines(app),
    };

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .scroll((app.details_scroll, 0));
    frame.render_widget(paragraph, chunks[1]);
}

fn detail_section_tabs(
    labels: &'static [(&'static str, usize)],
    active_index: usize,
) -> Line<'static> {
    let mut spans = Vec::new();
    for (idx, (label, key_hint)) in labels.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        }

        let style = if idx == active_index {
            Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let label = format!(" {}:{} ", key_hint, label);
        spans.push(Span::styled(label, style));
    }

    Line::from(spans)
}

pub(super) fn route_inspector_section_tabs(app: &App) -> Line<'static> {
    let active_index = match app.route_inspector.active_section {
        RouteInspectorSection::Summary => 0,
        RouteInspectorSection::PathViewer => 1,
        RouteInspectorSection::VpnRoutes => 2,
        RouteInspectorSection::Diagnostics => 3,
    };

    detail_section_tabs(
        &[("Summary", 1), ("Path", 2), ("VPN", 3), ("Diagnostics", 4)],
        active_index,
    )
}

pub(super) fn port_details_section_tabs(app: &App) -> Line<'static> {
    let active_index = match app.port_details_section {
        PortDetailsSection::Summary => 0,
        PortDetailsSection::Process => 1,
    };

    detail_section_tabs(&[("Summary", 1), ("Process", 2)], active_index)
}

pub(super) fn connection_details_section_tabs(app: &App) -> Line<'static> {
    let active_index = match app.connection_details_section {
        ConnectionDetailsSection::Summary => 0,
        ConnectionDetailsSection::Whois => 1,
    };

    detail_section_tabs(&[("Summary", 1), ("Whois", 2)], active_index)
}

fn is_remote_connection_target(ip: &str) -> bool {
    ip != "*" && ip != "::" && ip != "0.0.0.0" && ip != "*.*"
}

pub(super) fn resolve_connection_interface(app: &App, local_ip: &str) -> String {
    let mut mapped_interface = "N/A (External/Wildcard)".to_string();
    if let Some(snapshot) = &app.current_snapshot {
        for interface in &snapshot.interfaces {
            let matches_ipv4 = interface.ipv4.iter().any(|addr| addr.value == local_ip);
            let matches_ipv6 = interface.ipv6.iter().any(|addr| addr.value == local_ip);
            if matches_ipv4 || matches_ipv6 {
                mapped_interface =
                    format!("{} ({})", interface.name, interface.network_kind.as_str());
                break;
            }
        }
    }

    if local_ip == "127.0.0.1" || local_ip == "::1" || local_ip == "fe80::1%lo0" {
        "lo0 (LOOPBACK)".to_string()
    } else if local_ip == "*" || local_ip == "::" || local_ip == "0.0.0.0" {
        "All Interfaces (Wildcard)".to_string()
    } else {
        mapped_interface
    }
}

pub(super) struct ConnectionSummaryInput<'a> {
    pub proto: &'a str,
    pub local_ip_display: String,
    pub local_port: &'a str,
    pub foreign_ip_display: String,
    pub foreign_ip_raw: &'a str,
    pub foreign_port: &'a str,
    pub state: Option<&'a str>,
    pub mapped_interface: String,
}

pub(super) fn connection_summary_lines(input: ConnectionSummaryInput<'_>) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "=== Connection Summary ===",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Protocol:          ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(input.proto.to_uppercase().to_string()),
        ]),
        Line::from(vec![
            Span::styled(
                "Local IP:          ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(input.local_ip_display),
        ]),
        Line::from(vec![
            Span::styled(
                "Local Port:        ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(input.local_port.to_string()),
        ]),
        Line::from(vec![
            Span::styled(
                "Foreign IP:        ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(input.foreign_ip_display),
        ]),
        Line::from(vec![
            Span::styled(
                "Foreign Port:      ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(input.foreign_port.to_string()),
        ]),
    ];

    if let Some(state) = input.state {
        lines.push(Line::from(vec![
            Span::styled(
                "TCP State:         ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(state.to_string()),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled(
            "Associated Interface:",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}", input.mapped_interface)),
    ]));

    if is_remote_connection_target(input.foreign_ip_raw) {
        lines.push(Line::from("Press c: Copy IP | w: WHOIS Query"));
    }

    lines
}

pub(super) fn connection_whois_lines(app: &App, foreign_ip: &str) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "=== Whois ===",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    if !is_remote_connection_target(foreign_ip) {
        lines.push(Line::from("No remote peer to query."));
        return lines;
    }

    lines.push(Line::from(format!("Target: {}", foreign_ip)));
    match app.get_whois_result(foreign_ip).as_deref() {
        Some(whois_result) => {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Result:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            for line in whois_result.lines() {
                lines.push(Line::from(line.to_string()));
            }
        }
        None => {
            lines.push(Line::from("No WHOIS data loaded."));
            lines.push(Line::from("Press w to fetch WHOIS."));
        }
    }

    lines
}

pub(super) fn port_summary_lines(
    proto: &str,
    port: &str,
    pid: &str,
    user: &str,
) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "=== Port Summary ===",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Protocol: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(proto.to_uppercase().to_string()),
        ]),
        Line::from(vec![
            Span::styled("Port:     ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(port.to_string(), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("PID:      ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(pid.to_string()),
        ]),
        Line::from(vec![
            Span::styled("User:     ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(user.to_string()),
        ]),
        Line::from(""),
        Line::from("Press Tab for process details."),
    ]
}

pub(super) fn port_process_lines(command: &str, pid: &str, user: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "=== Process ===",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Command: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(command.to_string(), Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("PID:     ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(pid.to_string()),
        ]),
        Line::from(vec![
            Span::styled("User:    ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(user.to_string()),
        ]),
    ]
}

fn route_summary_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "=== Route Summary ===",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    let routes = app
        .current_snapshot
        .as_ref()
        .map(|snapshot| snapshot.routes.as_slice())
        .unwrap_or(&[]);

    if let Some(default_route) = routes.iter().find(|route| is_default_route(route)) {
        lines.push(Line::from(vec![
            Span::styled(
                "Default Gateway: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(default_route.gateway.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(
                "Default Interface: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                default_route.interface.clone(),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "No default route",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    }

    let ipv4_count = routes
        .iter()
        .filter(|route| route.family == RouteFamily::Ipv4)
        .count();
    let ipv6_count = routes
        .iter()
        .filter(|route| route.family == RouteFamily::Ipv6)
        .count();
    let first_vpn_interface = routes
        .iter()
        .find(|route| is_vpn_interface_name(&route.interface))
        .map(|route| route.interface.as_str());
    let warning_count = app
        .route_inspector
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == RouteDiagnosticSeverity::Warning)
        .count();

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            "IPv4 Routes: ",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(ipv4_count.to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled(
            "IPv6 Routes: ",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(ipv6_count.to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("VPN: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(
            if first_vpn_interface.is_some() {
                "Connected"
            } else {
                "Disconnected"
            },
            if first_vpn_interface.is_some() {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled(
            "First VPN Interface: ",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(first_vpn_interface.unwrap_or("None").to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Warnings: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(warning_count.to_string()),
    ]));

    lines
}

fn route_path_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "=== Path Viewer ===",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from({
            let mut spans = vec![
                Span::styled(
                    "Destination: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(app.route_inspector.destination_input.clone()),
            ];
            if app.route_inspector.destination_input_active {
                spans.push(Span::styled("█", Style::default().fg(Color::Yellow)));
            }
            spans
        }),
        Line::from(""),
    ];

    if let Some(result) = &app.route_inspector.latest_path_result {
        let graph = build_route_graph(result);
        lines.extend(render_route_graph_lines(&graph).into_iter().map(Line::from));
    } else if let Some(error) = &app.route_inspector.latest_path_error {
        lines.push(Line::from(Span::styled(
            error.clone(),
            Style::default().fg(Color::Red),
        )));
    } else {
        lines.push(Line::from(
            "Enter a destination and press Enter to inspect the route.",
        ));
    }

    lines
}

fn vpn_route_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "=== VPN Routes ===",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    let routes = app
        .current_snapshot
        .as_ref()
        .map(|snapshot| snapshot.routes.as_slice())
        .unwrap_or(&[]);
    let vpn_routes: Vec<_> = routes
        .iter()
        .filter(|route| is_vpn_interface_name(&route.interface))
        .collect();

    if vpn_routes.is_empty() {
        lines.push(Line::from("No VPN routes detected."));
        return lines;
    }

    for route in vpn_routes {
        lines.push(Line::from(vec![
            Span::styled(
                "Destination: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(route.destination.clone()),
            Span::raw("  "),
            Span::styled("Interface: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(route.interface.clone(), Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled("Gateway: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(route.gateway.clone()),
        ]));
    }

    lines
}

pub(super) fn route_diagnostic_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "=== Diagnostics ===",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    if app.route_inspector.diagnostics.is_empty() {
        lines.push(Line::from(Span::styled(
            "No routing warnings detected.",
            Style::default().fg(Color::Green),
        )));
        return lines;
    }

    for (index, diagnostic) in app.route_inspector.diagnostics.iter().enumerate() {
        if index > 0 {
            lines.push(Line::from(""));
        }
        let severity_style = Style::default().fg(diagnostic_color(diagnostic.severity));
        let severity_bold_style = severity_style.add_modifier(Modifier::BOLD);
        lines.push(Line::from(Span::styled(
            diagnostic.title.clone(),
            severity_bold_style,
        )));
        lines.push(Line::from(vec![
            Span::styled("Description: ", severity_bold_style),
            Span::styled(diagnostic.description.clone(), severity_style),
        ]));
        if let Some(route) = &diagnostic.affected_route {
            lines.push(Line::from(vec![
                Span::styled("Affected Route: ", severity_bold_style),
                Span::styled(
                    format!(
                        "{} via {} dev {} ({})",
                        route.destination,
                        route.gateway,
                        route.interface,
                        route_family_label(route.family),
                    ),
                    severity_style,
                ),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled("Recommendation: ", severity_bold_style),
            Span::styled(diagnostic.recommendation.clone(), severity_style),
        ]));
    }

    lines
}

pub(super) fn format_bps(bytes_per_sec: u64) -> String {
    if bytes_per_sec >= 1_000_000_000 {
        format!("{:.1} GB/s", bytes_per_sec as f64 / 1_000_000_000.0)
    } else if bytes_per_sec >= 1_000_000 {
        format!("{:.1} MB/s", bytes_per_sec as f64 / 1_000_000.0)
    } else if bytes_per_sec >= 1_000 {
        format!("{:.1} KB/s", bytes_per_sec as f64 / 1_000.0)
    } else {
        format!("{} B/s", bytes_per_sec)
    }
}
