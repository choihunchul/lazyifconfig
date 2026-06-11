use crate::app::App;
use crate::collector::connections::parse_connections;
use crate::collector::interface::{merge_gateways, parse_interfaces};
use crate::collector::ports::parse_listening_ports;
use crate::collector::routes::parse_routes;
use crate::collector::stats::merge_stats;
use crate::collector::system::collect_process_metrics;
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
use std::time::{SystemTime, UNIX_EPOCH};

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

    let interface_command = interface_command_spec();
    let raw_out_res = capture_command_output(
        app,
        CommandSourceId::Ifconfig,
        interface_command.display,
        interface_command.program,
        interface_command.args,
    );
    let raw_out = raw_out_res?;
    let mut parsed = parse_interfaces(&raw_out);

    let route_table_command = route_table_command_spec();
    let netstat_out_res = capture_command_output(
        app,
        CommandSourceId::NetstatRoutes,
        route_table_command.display,
        route_table_command.program,
        route_table_command.args,
    );
    let netstat_out = netstat_out_res.ok();
    if let Some(out) = &netstat_out {
        merge_gateways(&mut parsed, out);
    }

    let mut routes = if let Some(out) = &netstat_out {
        parse_routes(out)
    } else {
        Vec::new()
    };

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

    let stats_out = run_netstat_ib().unwrap_or_else(|_| raw_out.clone());
    let merged = merge_stats(&stats_out, parsed);

    let connections_res = capture_command_output(
        app,
        CommandSourceId::NetstatConnections,
        "netstat -an",
        "netstat",
        &["-an"],
    );
    let connections = if let Ok(netstat_an_out) = &connections_res {
        parse_connections(netstat_an_out)
    } else {
        Vec::new()
    };

    let listening_ports_command = listening_ports_command_spec();
    let ports_res = capture_command_output(
        app,
        CommandSourceId::LsofPorts,
        listening_ports_command.display,
        listening_ports_command.program,
        listening_ports_command.args,
    );
    let listening_ports = if let Ok(ports_out) = &ports_res {
        parse_listening_ports(ports_out)
    } else {
        Vec::new()
    };

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RouteFamily;

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

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn test_tick_update() {
        let mut app = App::default();
        let res = tick_update(&mut app);
        assert!(res.is_ok());
        assert!(app.current_snapshot.is_some());
    }
}
