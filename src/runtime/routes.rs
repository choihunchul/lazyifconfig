use crate::app::App;
use crate::collector::routes::{parse_linux_route_path, parse_macos_route_path};
use crate::command::{run_command_capture, CommandResult, OwnedCommandSpec};
use crate::model::{CommandOutput, CommandSourceId};

pub fn run_route_path_lookup(app: &mut App) {
    let destination = app.route_inspector.destination_input.trim().to_string();
    if destination.is_empty() {
        app.route_inspector.latest_path_result = None;
        app.route_inspector.latest_path_error = Some("Enter a destination first.".to_string());
        return;
    }

    let command = crate::command::route_path_command_spec(&destination);
    match capture_owned_command_output(app, CommandSourceId::RoutePath, &command) {
        Ok(output) => {
            let parsed = if cfg!(target_os = "linux") {
                parse_linux_route_path(&destination, &output)
            } else {
                parse_macos_route_path(&destination, &output)
            };

            match parsed {
                Ok(mut result) => {
                    result.is_vpn = result
                        .interface
                        .as_deref()
                        .map(crate::route_inspector::vpn::is_vpn_interface_name)
                        .unwrap_or(false);
                    app.route_inspector.latest_path_result = Some(result);
                    app.route_inspector.latest_path_error = None;
                }
                Err(error) => {
                    app.route_inspector.latest_path_result = None;
                    app.route_inspector.latest_path_error = Some(error);
                }
            }
        }
        Err(error) => {
            app.route_inspector.latest_path_result = None;
            app.route_inspector.latest_path_error = Some(route_path_command_error_message(&error));
        }
    }
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

fn route_path_command_error_message(error: &str) -> String {
    format!("destination could not be resolved by route command: {error}")
}

pub fn routes_raw_sources(app: &App) -> Vec<CommandSourceId> {
    let mut sources = vec![
        CommandSourceId::NetstatRoutes,
        CommandSourceId::DefaultRoute,
    ];
    if app
        .command_outputs
        .contains_key(&CommandSourceId::Ipv6Routes)
    {
        sources.push(CommandSourceId::Ipv6Routes);
    }
    if app.command_outputs.contains_key(&CommandSourceId::IpRules) {
        sources.push(CommandSourceId::IpRules);
    }
    if app
        .command_outputs
        .contains_key(&CommandSourceId::RoutePath)
    {
        sources.push(CommandSourceId::RoutePath);
    }
    if app.command_outputs.contains_key(&CommandSourceId::PublicIp) {
        sources.push(CommandSourceId::PublicIp);
    }
    sources
}

pub fn raw_viewer_command_to_copy(app: &App, src_id: CommandSourceId) -> String {
    app.command_outputs
        .get(&src_id)
        .map(|out| out.command.clone())
        .unwrap_or_else(|| src_id.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RoutePathResult;
    use std::time::SystemTime;

    #[test]
    fn route_path_lookup_requires_destination() {
        let mut app = App::default();
        app.route_inspector.destination_input = "   ".to_string();
        app.route_inspector.latest_path_result = Some(RoutePathResult {
            destination: "8.8.8.8".to_string(),
            ..Default::default()
        });

        run_route_path_lookup(&mut app);

        assert!(app.route_inspector.latest_path_result.is_none());
        assert_eq!(
            app.route_inspector.latest_path_error.as_deref(),
            Some("Enter a destination first.")
        );
    }

    #[test]
    fn route_path_command_error_message_uses_literal_destination_label() {
        assert_eq!(
            route_path_command_error_message("lookup failed"),
            "destination could not be resolved by route command: lookup failed"
        );
    }

    #[test]
    fn routes_raw_sources_include_available_optional_outputs_in_order() {
        let mut app = App::default();
        app.command_outputs.insert(
            CommandSourceId::RoutePath,
            CommandOutput {
                command: "ip route get 8.8.8.8".to_string(),
                stdout: String::new(),
                stderr: String::new(),
                executed_at: SystemTime::now(),
                exit_code: Some(0),
            },
        );
        app.command_outputs.insert(
            CommandSourceId::Ipv6Routes,
            CommandOutput {
                command: "ip -6 route show".to_string(),
                stdout: String::new(),
                stderr: String::new(),
                executed_at: SystemTime::now(),
                exit_code: Some(0),
            },
        );

        assert_eq!(
            routes_raw_sources(&app),
            vec![
                CommandSourceId::NetstatRoutes,
                CommandSourceId::DefaultRoute,
                CommandSourceId::Ipv6Routes,
                CommandSourceId::RoutePath,
            ]
        );
    }

    #[test]
    fn raw_viewer_command_to_copy_prefers_captured_command_and_falls_back_to_source_label() {
        let mut app = App::default();
        app.command_outputs.insert(
            CommandSourceId::RoutePath,
            CommandOutput {
                command: "ip route get 8.8.8.8".to_string(),
                stdout: String::new(),
                stderr: String::new(),
                executed_at: SystemTime::now(),
                exit_code: Some(0),
            },
        );

        assert_eq!(
            raw_viewer_command_to_copy(&app, CommandSourceId::RoutePath),
            "ip route get 8.8.8.8"
        );
        assert_eq!(
            raw_viewer_command_to_copy(&app, CommandSourceId::Ifconfig),
            CommandSourceId::Ifconfig.as_str()
        );
    }
}
