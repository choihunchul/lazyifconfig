use crate::app::App;
use crate::app::ViewMode;
use crate::collector::connections::parse_connections;
use crate::collector::interface::{merge_gateways, parse_interfaces};
use crate::collector::ports::{enrich_listening_ports_with_processes, parse_listening_ports};
use crate::collector::routes::parse_routes;
use crate::collector::stats::merge_stats;
use crate::collector::system::{
    collect_process_details, collect_process_metrics, collect_windows_process_details_by_pid,
};
use crate::collector::windows::{
    merge_powershell_interface_stats, parse_powershell_connections, parse_powershell_interfaces,
    parse_powershell_listening_ports, parse_powershell_routes,
};
use crate::command::{
    default_route_command_spec, interface_command_spec, listening_ports_command_spec,
    route_table_command_spec, run_command_capture, run_netstat_ib, CommandResult, OwnedCommandSpec,
};
use crate::model::{
    CommandOutput, CommandSourceId, EventSeverity, NetworkEvent, NetworkEventKind, NetworkSnapshot,
    PublicIpInfo, RouteEntry,
};
use crate::runtime::update_flow::{
    drain_update_messages, maybe_start_auto_update_check, maybe_start_auto_update_install,
};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

const WINDOWS_INTERFACE_STATS_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

pub fn tick_update(app: &mut App) -> Result<(), String> {
    // Merge async command outputs
    if let Ok(lock) = app.async_command_outputs.lock() {
        for (k, v) in lock.iter() {
            app.command_outputs.insert(*k, v.clone());
        }
    }

    drain_update_messages(app);
    maybe_start_auto_update_check(app);
    maybe_start_auto_update_install(app);

    app.process_metrics = Some(collect_process_metrics());

    let (raw_out, mut parsed) = collect_interfaces(app)?;

    let netstat_out_res = collect_route_table_output(app);
    let netstat_out = netstat_out_res.ok();
    if let Some(out) = &netstat_out {
        merge_gateways(&mut parsed, out);
    }

    let mut routes = collect_routes(netstat_out.as_deref());

    let default_route_command = default_route_command_spec();
    let _ = capture_command_output(
        app,
        CommandSourceId::DefaultRoute,
        default_route_command.display,
        default_route_command.program,
        default_route_command.args,
    );

    if let Some(command) = crate::command::ipv6_route_table_command_spec() {
        let ipv6_route_out =
            capture_owned_command_output(app, CommandSourceId::Ipv6Routes, &command).ok();
        routes = merge_additional_route_output(routes, ipv6_route_out.as_deref());
    }

    if let Some(command) = crate::command::ip_rule_command_spec() {
        let _ = capture_owned_command_output(app, CommandSourceId::IpRules, &command);
    }

    let merged = collect_interface_stats(app, &raw_out, parsed);

    let connections = collect_connections(app);

    let listening_ports = collect_or_reuse_listening_ports(app);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // --- Background Public IP Fetching ---
    let should_fetch = match app.last_public_ip_fetch {
        None => true,
        Some(last) => last.elapsed() >= std::time::Duration::from_secs(300),
    };

    if should_fetch {
        app.last_public_ip_fetch = Some(std::time::Instant::now());
        let public_ip_info_clone = app.public_ip_info.clone();
        let async_outputs_clone = app.async_command_outputs.clone();
        tokio::spawn(async move {
            let start_time = std::time::SystemTime::now();
            let raw_json_capture =
                run_command_capture("curl", &["-s", "-m", "5", "https://ipinfo.io/json"]);
            let raw_json_res = raw_json_capture
                .as_ref()
                .map(command_stdout)
                .unwrap_or_else(|e| Err(e.clone()));

            if let Ok(mut lock) = async_outputs_clone.lock() {
                lock.insert(
                    CommandSourceId::PublicIp,
                    CommandOutput {
                        command: "curl -s -m 5 https://ipinfo.io/json".to_string(),
                        stdout: raw_json_capture
                            .as_ref()
                            .map(|out| out.stdout.clone())
                            .unwrap_or_default(),
                        stderr: raw_json_capture
                            .as_ref()
                            .map(|out| out.stderr.clone())
                            .unwrap_or_else(|e| e.clone()),
                        executed_at: start_time,
                        exit_code: raw_json_capture
                            .as_ref()
                            .ok()
                            .and_then(|out| out.exit_code)
                            .or(Some(1)),
                    },
                );
            }

            if let Ok(raw_json) = raw_json_res {
                #[derive(serde::Deserialize)]
                struct IpInfoResponse {
                    ip: String,
                    org: Option<String>,
                    country: Option<String>,
                }
                if let Ok(res) = serde_json::from_str::<IpInfoResponse>(&raw_json) {
                    let info = PublicIpInfo {
                        ip: res.ip,
                        provider: res.org,
                        country: res.country,
                    };
                    if let Ok(mut lock) = public_ip_info_clone.lock() {
                        *lock = Some(info);
                    }
                }
            }
        });
    }

    // --- Public IP Change Detection ---
    if let Ok(lock) = app.public_ip_info.lock() {
        if let Some(new_info) = &*lock {
            let mut changed = false;
            let mut ip_changed_msg = None;
            let mut prov_changed_msg = None;

            if let Some(old_info) = &app.current_public_ip_info {
                if old_info.ip != new_info.ip {
                    ip_changed_msg = Some(format!(
                        "Public IP Changed: {} -> {}",
                        old_info.ip, new_info.ip
                    ));
                    changed = true;
                }
                if old_info.provider != new_info.provider {
                    prov_changed_msg = Some(format!(
                        "Provider Changed: {} -> {}",
                        old_info.provider.as_deref().unwrap_or("Unknown"),
                        new_info.provider.as_deref().unwrap_or("Unknown")
                    ));
                    changed = true;
                }
            } else {
                changed = true;
            }

            if changed {
                if let Some(msg) = ip_changed_msg {
                    app.recent_events.push(NetworkEvent::new(
                        NetworkEventKind::PublicIpChanged,
                        EventSeverity::Info,
                        msg,
                    ));
                }
                if let Some(msg) = prov_changed_msg {
                    app.recent_events.push(NetworkEvent::new(
                        NetworkEventKind::ProviderChanged,
                        EventSeverity::Info,
                        msg,
                    ));
                }
                app.current_public_ip_info = Some(new_info.clone());
            }
        }
    }

    app.replace_snapshot(NetworkSnapshot {
        interfaces: merged,
        connections,
        listening_ports,
        routes,
        captured_at_secs: now,
    });
    Ok(())
}

const POWERSHELL_INTERFACES_COMMAND: &str = "$ErrorActionPreference='Stop'; Get-NetIPConfiguration | ForEach-Object { $adapter = Get-NetAdapter -InterfaceIndex $_.InterfaceIndex -ErrorAction SilentlyContinue; [pscustomobject]@{ InterfaceAlias=$_.InterfaceAlias; InterfaceIndex=$_.InterfaceIndex; InterfaceDescription=$_.InterfaceDescription; NetAdapter=$adapter; IPv4Address=$_.IPv4Address; IPv6Address=$_.IPv6Address; IPv4DefaultGateway=$_.IPv4DefaultGateway; IPv6DefaultGateway=$_.IPv6DefaultGateway } } | ConvertTo-Json -Depth 6 -Compress";
const POWERSHELL_ROUTES_COMMAND: &str = "$ErrorActionPreference='Stop'; Get-NetRoute | Select-Object DestinationPrefix,NextHop,InterfaceAlias,InterfaceIndex,RouteMetric,Protocol,AddressFamily | ConvertTo-Json -Depth 4 -Compress";
const POWERSHELL_CONNECTIONS_COMMAND: &str = "$ErrorActionPreference='Stop'; Get-NetTCPConnection | Select-Object LocalAddress,LocalPort,RemoteAddress,RemotePort,State | ConvertTo-Json -Depth 4 -Compress";
const POWERSHELL_PORTS_COMMAND: &str = "$ErrorActionPreference='Stop'; Get-NetTCPConnection -State Listen | Select-Object LocalAddress,LocalPort,OwningProcess | ConvertTo-Json -Depth 4 -Compress";
const POWERSHELL_INTERFACE_STATS_COMMAND: &str = "$ErrorActionPreference='Stop'; Get-NetAdapterStatistics | Select-Object Name,ReceivedBytes,SentBytes,ReceivedUnicastPackets,SentUnicastPackets | ConvertTo-Json -Depth 4 -Compress";

fn collect_interfaces(
    app: &mut App,
) -> Result<(String, Vec<crate::model::NetworkInterface>), String> {
    if cfg!(target_os = "windows") {
        let interface_command = interface_command_spec();
        match capture_command_output(
            app,
            CommandSourceId::Ifconfig,
            interface_command.display,
            interface_command.program,
            interface_command.args,
        )
        .and_then(|output| {
            let parsed = parse_interfaces(&output);
            if parsed.is_empty() {
                Err("ipconfig returned no parsed interface data".to_string())
            } else {
                Ok((output, parsed))
            }
        }) {
            Ok(result) => return Ok(result),
            Err(error) => push_windows_fallback_event(app, "interfaces", "PowerShell", &error),
        }

        return capture_powershell_output(
            app,
            CommandSourceId::Ifconfig,
            POWERSHELL_INTERFACES_COMMAND,
        )
        .and_then(|output| {
            let parsed = parse_powershell_interfaces(&output);
            if parsed.is_empty() {
                Err("PowerShell returned no interface data".to_string())
            } else {
                Ok((output, parsed))
            }
        });
    }

    let interface_command = interface_command_spec();
    let raw_out = capture_command_output(
        app,
        CommandSourceId::Ifconfig,
        interface_command.display,
        interface_command.program,
        interface_command.args,
    )?;
    let parsed = parse_interfaces(&raw_out);
    Ok((raw_out, parsed))
}

fn collect_route_table_output(app: &mut App) -> Result<String, String> {
    if cfg!(target_os = "windows") {
        let route_table_command = route_table_command_spec();
        match capture_command_output(
            app,
            CommandSourceId::NetstatRoutes,
            route_table_command.display,
            route_table_command.program,
            route_table_command.args,
        )
        .and_then(|output| {
            if parse_routes(&output).is_empty() {
                Err("route PRINT returned no parsed route data".to_string())
            } else {
                Ok(output)
            }
        }) {
            Ok(output) => return Ok(output),
            Err(error) => push_windows_fallback_event(app, "routes", "PowerShell", &error),
        }

        return capture_powershell_output(
            app,
            CommandSourceId::NetstatRoutes,
            POWERSHELL_ROUTES_COMMAND,
        )
        .and_then(|output| {
            if parse_powershell_routes(&output).is_empty() {
                Err("PowerShell returned no route data".to_string())
            } else {
                Ok(output)
            }
        });
    }

    let route_table_command = route_table_command_spec();
    capture_command_output(
        app,
        CommandSourceId::NetstatRoutes,
        route_table_command.display,
        route_table_command.program,
        route_table_command.args,
    )
}

fn collect_interface_stats(
    app: &mut App,
    raw_out: &str,
    parsed: Vec<crate::model::NetworkInterface>,
) -> Vec<crate::model::NetworkInterface> {
    if cfg!(target_os = "windows") {
        let now = std::time::Instant::now();
        if !should_collect_windows_interface_stats(app.last_interface_stats_fetch, now) {
            return reuse_previous_interface_stats(
                parsed,
                app.current_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.interfaces.as_slice())
                    .unwrap_or(&[]),
            );
        }
        app.last_interface_stats_fetch = Some(now);

        match capture_powershell_output(
            app,
            CommandSourceId::InterfaceStats,
            POWERSHELL_INTERFACE_STATS_COMMAND,
        ) {
            Ok(output) => return merge_powershell_interface_stats(&output, parsed),
            Err(error) => {
                app.push_event(NetworkEvent::new(
                    NetworkEventKind::SystemError,
                    EventSeverity::Warning,
                    format!(
                        "interface stats: Windows PowerShell statistics unavailable; RX/TX graph may be empty. Cause: {error}"
                    ),
                ));
                return parsed;
            }
        }
    }

    let stats_out = run_netstat_ib().unwrap_or_else(|_| raw_out.to_string());
    merge_stats(&stats_out, parsed)
}

fn should_collect_windows_interface_stats(
    last_fetch: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    last_fetch
        .is_none_or(|last_fetch| now.duration_since(last_fetch) >= WINDOWS_INTERFACE_STATS_INTERVAL)
}

fn reuse_previous_interface_stats(
    mut parsed: Vec<crate::model::NetworkInterface>,
    previous: &[crate::model::NetworkInterface],
) -> Vec<crate::model::NetworkInterface> {
    for interface in &mut parsed {
        if interface.stats.is_none() {
            interface.stats = previous
                .iter()
                .find(|previous| previous.name == interface.name)
                .and_then(|previous| previous.stats.clone());
        }
    }
    parsed
}

fn collect_routes(route_output: Option<&str>) -> Vec<RouteEntry> {
    if cfg!(target_os = "windows") {
        if let Some(output) = route_output {
            let routes = parse_powershell_routes(output);
            if !routes.is_empty() {
                return routes;
            }
        }
    }

    route_output.map(parse_routes).unwrap_or_default()
}

fn collect_connections(app: &mut App) -> Vec<crate::model::ActiveConnection> {
    if cfg!(target_os = "windows") {
        match capture_command_output(
            app,
            CommandSourceId::NetstatConnections,
            "netstat -an",
            "netstat",
            &["-an"],
        )
        .and_then(|output| {
            let connections = parse_connections(&output);
            if connections.is_empty() {
                Err("netstat -an returned no parsed connection data".to_string())
            } else {
                Ok(connections)
            }
        }) {
            Ok(connections) => return connections,
            Err(error) => push_windows_fallback_event(app, "connections", "PowerShell", &error),
        }

        return capture_powershell_output(
            app,
            CommandSourceId::NetstatConnections,
            POWERSHELL_CONNECTIONS_COMMAND,
        )
        .as_deref()
        .map(parse_powershell_connections)
        .unwrap_or_default();
    }

    let connections_res = capture_command_output(
        app,
        CommandSourceId::NetstatConnections,
        "netstat -an",
        "netstat",
        &["-an"],
    );
    connections_res
        .as_deref()
        .map(parse_connections)
        .unwrap_or_default()
}

fn collect_listening_ports(app: &mut App) -> Vec<crate::model::ListeningPort> {
    if cfg!(target_os = "windows") {
        let listening_ports_command = listening_ports_command_spec();
        match capture_command_output(
            app,
            CommandSourceId::LsofPorts,
            listening_ports_command.display,
            listening_ports_command.program,
            listening_ports_command.args,
        )
        .and_then(|output| {
            let ports = parse_listening_ports(&output);
            if ports.is_empty() {
                Err("netstat -ano -p tcp returned no parsed listening port data".to_string())
            } else {
                Ok(ports)
            }
        }) {
            Ok(ports) => return ports,
            Err(error) => push_windows_fallback_event(app, "ports", "PowerShell", &error),
        }

        return capture_powershell_output(
            app,
            CommandSourceId::LsofPorts,
            POWERSHELL_PORTS_COMMAND,
        )
        .as_deref()
        .map(parse_powershell_listening_ports)
        .unwrap_or_default();
    }

    let listening_ports_command = listening_ports_command_spec();
    let ports_res = capture_command_output(
        app,
        CommandSourceId::LsofPorts,
        listening_ports_command.display,
        listening_ports_command.program,
        listening_ports_command.args,
    );
    ports_res
        .as_deref()
        .map(parse_listening_ports)
        .unwrap_or_default()
}

fn collect_or_reuse_listening_ports(app: &mut App) -> Vec<crate::model::ListeningPort> {
    if !should_collect_listening_ports(app.view_mode) {
        return app
            .current_snapshot
            .as_ref()
            .map(|snapshot| snapshot.listening_ports.clone())
            .unwrap_or_default();
    }

    let detail_pid =
        selected_port_detail_pid(app).filter(|_| should_collect_port_process_details(app));
    let cached_detail = detail_pid
        .as_deref()
        .and_then(|pid| cached_port_process_details(app, pid));
    let mut listening_ports = collect_listening_ports(app);
    enrich_windows_listening_ports(&mut listening_ports);
    let detail_pid = detail_pid.or_else(|| {
        should_collect_port_process_details(app)
            .then(|| {
                listening_ports
                    .first()
                    .map(|port| port.pid.clone())
                    .filter(|pid| !pid.is_empty() && pid != "-")
            })
            .flatten()
    });
    if let (Some(detail_pid), Some(cached_detail)) = (detail_pid.as_deref(), cached_detail) {
        attach_process_details(&mut listening_ports, detail_pid, cached_detail);
    } else {
        enrich_listening_port_process_details(&mut listening_ports, detail_pid.as_deref());
    }
    listening_ports
}

fn should_collect_listening_ports(view_mode: ViewMode) -> bool {
    view_mode == ViewMode::Ports
}

fn should_collect_port_process_details(app: &App) -> bool {
    app.view_mode == ViewMode::Ports
        && app.port_details_section == crate::app::PortDetailsSection::Detail
}

fn selected_port_detail_pid(app: &App) -> Option<String> {
    match app.navigation_items.get(app.selected_index) {
        Some(crate::app::NavigationItem::ListeningPort { pid, .. }) => Some(pid.clone()),
        _ => None,
    }
    .filter(|pid| !pid.is_empty() && pid != "-")
}

fn cached_port_process_details(app: &App, pid: &str) -> Option<crate::model::ProcessDetails> {
    app.current_snapshot
        .as_ref()?
        .listening_ports
        .iter()
        .find(|port| port.pid == pid)
        .and_then(|port| port.process.clone())
}

fn capture_powershell_output(
    app: &mut App,
    source_id: CommandSourceId,
    command: &str,
) -> Result<String, String> {
    capture_command_output(
        app,
        source_id,
        &format!("powershell.exe -NoProfile -Command {command}"),
        "powershell.exe",
        &["-NoProfile", "-Command", command],
    )
}

fn push_windows_fallback_event(app: &mut App, area: &str, fallback: &str, error: &str) {
    let message = format!(
        "{area}: Windows command failed or returned no parsed data; falling back to {fallback}. If command output uses an unsupported language or permissions are required, PowerShell network cmdlets may still work. Cause: {error}"
    );
    if app
        .recent_events
        .last()
        .is_none_or(|event| event.message != message)
    {
        app.recent_events.push(NetworkEvent::new(
            NetworkEventKind::SystemError,
            EventSeverity::Warning,
            message,
        ));
    }
}

fn enrich_windows_listening_ports(listening_ports: &mut [crate::model::ListeningPort]) {
    if !cfg!(target_os = "windows") || listening_ports.is_empty() {
        return;
    }

    let Ok(output) = run_command_capture("tasklist", &["/fo", "csv", "/nh"]) else {
        return;
    };
    let Ok(stdout) = command_stdout(&output) else {
        return;
    };

    enrich_listening_ports_with_processes(listening_ports, &stdout);
}

fn capture_command_output(
    app: &mut App,
    source_id: CommandSourceId,
    command: &str,
    program: &str,
    args: &[&str],
) -> Result<String, String> {
    let captured = run_command_capture(program, args)?;
    let result = command_stdout(&captured);
    app.command_outputs.insert(
        source_id,
        CommandOutput {
            command: command.to_string(),
            stdout: captured.stdout,
            stderr: captured.stderr,
            executed_at: std::time::SystemTime::now(),
            exit_code: captured.exit_code,
        },
    );
    result
}

fn capture_owned_command_output(
    app: &mut App,
    source_id: CommandSourceId,
    command: &OwnedCommandSpec,
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

fn command_stdout(output: &CommandResult) -> Result<String, String> {
    if output.exit_code == Some(0) {
        Ok(output.stdout.clone())
    } else if output.stderr.trim().is_empty() {
        Err(format!("command exited with {:?}", output.exit_code))
    } else {
        Err(output.stderr.clone())
    }
}

fn merge_additional_route_output(
    mut routes: Vec<RouteEntry>,
    additional_output: Option<&str>,
) -> Vec<RouteEntry> {
    if let Some(output) = additional_output {
        routes.extend(parse_routes(output));
    }
    routes
}

fn enrich_listening_port_process_details(
    ports: &mut [crate::model::ListeningPort],
    detail_pid: Option<&str>,
) {
    let Some(detail_pid) = detail_pid else {
        return;
    };

    if cfg!(target_os = "windows") {
        let pids = vec![detail_pid.to_string()];
        let details_by_pid = collect_windows_process_details_by_pid(&pids);
        if let Some(details) = details_by_pid.get(detail_pid).cloned() {
            attach_process_details(ports, detail_pid, details);
        }
        return;
    }

    let mut details_by_pid = HashMap::new();
    for port in ports.iter_mut() {
        if port.pid != detail_pid {
            continue;
        }
        if port.process.is_some() {
            continue;
        }

        let details = details_by_pid
            .entry(port.pid.clone())
            .or_insert_with(|| collect_process_details(&port.pid));
        port.process = details.clone();
    }
}

fn attach_process_details(
    ports: &mut [crate::model::ListeningPort],
    detail_pid: &str,
    details: crate::model::ProcessDetails,
) {
    for port in ports.iter_mut() {
        if port.pid == detail_pid && port.process.is_none() {
            port.process = Some(details.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ListeningPort, ProcessDetails, RouteFamily};

    #[test]
    fn additional_linux_ipv6_route_output_is_merged_into_snapshot_routes() {
        let routes = parse_routes("default via 172.17.0.1 dev eth0 proto static metric 100");

        let merged = merge_additional_route_output(
            routes,
            Some("default via fe80::1 dev eth0 proto ra metric 100\n2001:db8::/64 dev eth0 proto kernel metric 256"),
        );

        assert!(merged.iter().any(|route| route.family == RouteFamily::Ipv6));
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn listening_ports_are_collected_only_for_ports_view() {
        assert!(should_collect_listening_ports(ViewMode::Ports));
        assert!(!should_collect_listening_ports(ViewMode::Interface));
        assert!(!should_collect_listening_ports(ViewMode::Network));
        assert!(!should_collect_listening_ports(ViewMode::Connections));
        assert!(!should_collect_listening_ports(ViewMode::Routes));
        assert!(!should_collect_listening_ports(ViewMode::Tools));
        assert!(!should_collect_listening_ports(ViewMode::Timeline));
    }

    #[test]
    fn selected_port_detail_pid_uses_current_navigation_item() {
        let mut app = App::default();
        app.navigation_items = vec![crate::app::NavigationItem::ListeningPort {
            proto: "tcp".to_string(),
            port: "5050".to_string(),
            command: "python.exe".to_string(),
            pid: "2460".to_string(),
            user: "-".to_string(),
            index: 0,
        }];

        assert_eq!(selected_port_detail_pid(&app).as_deref(), Some("2460"));
    }

    #[test]
    fn cached_process_details_are_reused_for_selected_pid() {
        let mut app = App::default();
        app.current_snapshot = Some(crate::model::NetworkSnapshot {
            interfaces: vec![],
            connections: vec![],
            listening_ports: vec![ListeningPort {
                proto: "tcp".to_string(),
                local_ip: "127.0.0.1".to_string(),
                local_port: "5050".to_string(),
                pid: "2460".to_string(),
                command: "python.exe".to_string(),
                user: "-".to_string(),
                process: Some(ProcessDetails {
                    executable: Some("C:\\Python312\\python.exe".to_string()),
                    command_line: Some("python -m http.server 5050".to_string()),
                    ..ProcessDetails::default()
                }),
            }],
            routes: vec![],
            captured_at_secs: 0,
        });

        let cached = cached_port_process_details(&app, "2460").expect("cached process");

        assert_eq!(
            cached.executable.as_deref(),
            Some("C:\\Python312\\python.exe")
        );
        assert_eq!(
            cached.command_line.as_deref(),
            Some("python -m http.server 5050")
        );
    }

    #[test]
    fn windows_interface_stats_collection_is_throttled() {
        let now = std::time::Instant::now();

        assert!(should_collect_windows_interface_stats(None, now));
        assert!(!should_collect_windows_interface_stats(
            Some(now - std::time::Duration::from_secs(9)),
            now
        ));
        assert!(should_collect_windows_interface_stats(
            Some(now - std::time::Duration::from_secs(10)),
            now
        ));
    }

    #[test]
    fn previous_interface_stats_are_reused_by_name() {
        let previous_stats = crate::model::InterfaceStats {
            rx_bytes: 1000,
            tx_bytes: 2000,
            rx_packets: 10,
            tx_packets: 20,
        };
        let previous = vec![crate::model::NetworkInterface {
            name: "Wi-Fi".to_string(),
            network_kind: crate::model::NetworkKind::Lan,
            interface_type: crate::model::InterfaceType::WifiOrEthernet,
            status: crate::model::InterfaceStatus::Up,
            ipv4: vec![],
            ipv6: vec![],
            mac_address: None,
            mtu: None,
            stats: Some(previous_stats.clone()),
        }];
        let parsed = vec![crate::model::NetworkInterface {
            name: "Wi-Fi".to_string(),
            network_kind: crate::model::NetworkKind::Lan,
            interface_type: crate::model::InterfaceType::WifiOrEthernet,
            status: crate::model::InterfaceStatus::Up,
            ipv4: vec![],
            ipv6: vec![],
            mac_address: None,
            mtu: None,
            stats: None,
        }];

        let reused = reuse_previous_interface_stats(parsed, &previous);

        assert_eq!(reused[0].stats.as_ref(), Some(&previous_stats));
    }

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn test_tick_update() {
        let mut app = App::default();
        let res = tick_update(&mut app);
        assert!(res.is_ok());
        assert!(app.current_snapshot.is_some());
    }
}
