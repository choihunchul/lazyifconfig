use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lazyifconfig::app::{App, NavigationItem, PortProcessAction, ViewMode};
use lazyifconfig::command::{run_kill, run_restart_process};
use lazyifconfig::model::{
    CommandOutput, CommandSourceId, EventSeverity, NetworkEvent, NetworkEventKind,
    RouteInspectorSection,
};
use lazyifconfig::runtime::refresh::tick_update;
use lazyifconfig::runtime::routes::{
    raw_viewer_command_to_copy, routes_raw_sources, run_route_path_lookup,
};
use lazyifconfig::runtime::update_flow::{start_update_check, start_update_install};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;

fn start_selected_tool(app: &mut App) {
    let tool_id = app.tools.selected_tool_id();
    if !app.tools.selected_tool_is_runnable() {
        app.tools.errors.insert(
            tool_id,
            "This tool is planned and is not executable yet.".to_string(),
        );
        app.tools
            .states
            .insert(tool_id, lazyifconfig::tools::ToolExecutionState::Failed);
        return;
    }

    let validation_errors = app.tools.selected_input_validation_errors();
    if !validation_errors.is_empty() {
        app.tools
            .errors
            .insert(tool_id, validation_errors.join("\n"));
        app.tools
            .states
            .insert(tool_id, lazyifconfig::tools::ToolExecutionState::Failed);
        return;
    }

    app.tools.expand_dns_raw_output();

    let input = app.tools.input_for_selected_tool().clone();
    app.tools.errors.remove(&tool_id);
    app.tools
        .states
        .insert(tool_id, lazyifconfig::tools::ToolExecutionState::Running);

    let pending = app.pending_tool_results.clone();
    tokio::spawn(async move {
        let result = lazyifconfig::tools::run_tool(tool_id, input).await;
        if let Ok(mut lock) = pending.lock() {
            lock.push((tool_id, result));
        }
    });
}

fn should_refresh_immediately_after_port_navigation() -> bool {
    !cfg!(target_os = "windows")
}

struct RefreshTask {
    base_event_count: usize,
    show_all: bool,
    handle: tokio::task::JoinHandle<Result<App, String>>,
}

fn start_refresh_task(app: &App) -> RefreshTask {
    let mut refreshed = app.clone();
    let base_event_count = app.recent_events.len();
    let show_all = app.show_all;
    let handle = tokio::task::spawn_blocking(move || {
        tick_update(&mut refreshed)?;
        Ok(refreshed)
    });

    RefreshTask {
        base_event_count,
        show_all,
        handle,
    }
}

fn request_refresh(
    app: &App,
    refresh_task: &mut Option<RefreshTask>,
    last_tick: &mut std::time::Instant,
) {
    if refresh_task.is_none() {
        *refresh_task = Some(start_refresh_task(app));
    }
    *last_tick = std::time::Instant::now();
}

async fn apply_finished_refresh_task(app: &mut App, refresh_task: &mut Option<RefreshTask>) {
    if !refresh_task
        .as_ref()
        .is_some_and(|task| task.handle.is_finished())
    {
        return;
    }

    let task = refresh_task.take().expect("checked refresh task presence");
    match task.handle.await {
        Ok(Ok(refreshed)) => {
            if task.show_all == app.show_all {
                apply_refresh_result(app, refreshed, task.base_event_count);
            }
        }
        Ok(Err(error)) => app.push_event(NetworkEvent::new(
            NetworkEventKind::SystemError,
            EventSeverity::Error,
            format!("Refresh failed: {error}"),
        )),
        Err(error) => app.push_event(NetworkEvent::new(
            NetworkEventKind::SystemError,
            EventSeverity::Error,
            format!("Refresh task failed: {error}"),
        )),
    }
}

fn apply_refresh_result(app: &mut App, refreshed: App, base_event_count: usize) {
    let selected_interface = app.selected_interface_name().map(str::to_owned);
    let refresh_events = refreshed
        .recent_events
        .into_iter()
        .skip(base_event_count)
        .collect::<Vec<_>>();

    app.current_snapshot = refreshed.current_snapshot;
    app.previous_snapshot = refreshed.previous_snapshot;
    app.traffic_history = refreshed.traffic_history;
    merge_refresh_command_outputs(app, refreshed.command_outputs);
    app.process_metrics = refreshed.process_metrics;
    app.current_public_ip_info = refreshed.current_public_ip_info;
    app.last_public_ip_fetch = refreshed.last_public_ip_fetch;
    app.last_interface_stats_fetch = refreshed.last_interface_stats_fetch;
    app.update_status = refreshed.update_status;
    app.pending_update = refreshed.pending_update;
    app.last_update_check = refreshed.last_update_check;
    app.attempted_update_version = refreshed.attempted_update_version;
    app.latest_release_date = refreshed.latest_release_date;
    app.route_inspector.diagnostics = refreshed.route_inspector.diagnostics;
    app.recent_events.extend(refresh_events);
    if app.recent_events.len() > 100 {
        let overflow = app.recent_events.len() - 100;
        app.recent_events.drain(0..overflow);
    }

    app.update_navigation_items();
    if let Some(name) = selected_interface {
        if let Some(index) = app
            .navigation_items
            .iter()
            .position(|item| matches!(item, NavigationItem::Interface { name: item_name, .. } if item_name == &name))
        {
            app.selected_index = index;
        }
    }
}

fn merge_refresh_command_outputs(
    app: &mut App,
    refreshed: std::collections::HashMap<CommandSourceId, CommandOutput>,
) {
    const REFRESH_SOURCES: &[CommandSourceId] = &[
        CommandSourceId::Ifconfig,
        CommandSourceId::NetstatRoutes,
        CommandSourceId::DefaultRoute,
        CommandSourceId::Ipv6Routes,
        CommandSourceId::IpRules,
        CommandSourceId::NetstatConnections,
        CommandSourceId::LsofPorts,
        CommandSourceId::InterfaceStats,
        CommandSourceId::PublicIp,
        CommandSourceId::GitHubRelease,
    ];

    for source in REFRESH_SOURCES {
        if let Some(output) = refreshed.get(source) {
            app.command_outputs.insert(*source, output.clone());
        }
    }
}

async fn run_tools_cli_command(args: &[String]) -> Result<String, String> {
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        return Ok(lazyifconfig::tools::tools_cli_usage());
    }

    let tool_name = &args[0];
    let tool_id = lazyifconfig::tools::tool_id_from_cli_name(tool_name)
        .ok_or_else(|| format!("Unknown tool: {tool_name}"))?;
    let tool_args = args[1..].iter().map(String::as_str).collect::<Vec<_>>();
    let input = lazyifconfig::tools::tool_input_from_cli_args(tool_id, &tool_args)?;
    let result = lazyifconfig::tools::run_tool(tool_id, input).await?;
    Ok(lazyifconfig::tools::format_tool_result_plaintext(&result))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_args = std::env::args().skip(1).collect::<Vec<_>>();
    if cli_args.first().map(String::as_str) == Some("tools") {
        match run_tools_cli_command(&cli_args[1..]).await {
            Ok(output) => {
                println!("{output}");
                return Ok(());
            }
            Err(error) => {
                eprintln!("{error}");
                eprintln!();
                eprintln!("{}", lazyifconfig::tools::tools_cli_usage());
                std::process::exit(2);
            }
        }
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::default();
    let _ = tick_update(&mut app);

    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_secs(2);
    let mut restart_requested = false;
    let mut refresh_task: Option<RefreshTask> = None;

    loop {
        apply_finished_refresh_task(&mut app, &mut refresh_task).await;
        app.drain_pending_tool_results();
        terminal.draw(|f| lazyifconfig::ui::draw(f, &app))?;

        let mut timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::from_secs(0));
        if refresh_task.is_some() {
            timeout = timeout.min(Duration::from_millis(50));
        }

        if event::poll(timeout)? {
            match event::read()? {
                Event::Paste(text) => {
                    if app.view_mode == ViewMode::Tools && app.tools.input_modal_open {
                        app.tools.push_input_text(&text);
                    }
                    continue;
                }
                Event::Key(key) => {
                    if !lazyifconfig::input::should_handle_key_event(key) {
                        continue;
                    }

                    let is_ctrl_c = matches!(key.code, KeyCode::Char('c' | 'C'))
                        && key.modifiers.contains(KeyModifiers::CONTROL);

                    if app.quit_confirmation_active {
                        match key.code {
                            KeyCode::Esc
                            | KeyCode::Char('n')
                            | KeyCode::Char('N')
                            | KeyCode::Char('q')
                            | KeyCode::Char('ㅂ') => {
                                app.cancel_quit_confirmation();
                            }
                            _ if is_ctrl_c => break,
                            _ => {}
                        }
                        continue;
                    }

                    if is_ctrl_c {
                        app.arm_quit_confirmation();
                        continue;
                    }

                    if app.pending_port_action.is_some() {
                        match key.code {
                            KeyCode::Esc
                            | KeyCode::Char('q')
                            | KeyCode::Char('ㅂ')
                            | KeyCode::Char('n')
                            | KeyCode::Char('N') => {
                                app.cancel_pending_port_action();
                            }
                            KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::Char('ㄱ') => {
                                app.set_pending_port_action(PortProcessAction::Restart);
                            }
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                if let Some(confirmation) = app.pending_port_action.take() {
                                    let result = match confirmation.action {
                                        PortProcessAction::Kill => run_kill(&confirmation.pid),
                                        PortProcessAction::Restart => {
                                            run_restart_process(&confirmation.pid)
                                        }
                                    };
                                    match result {
                                        Ok(()) => {
                                            let (kind, message) = match confirmation.action {
                                                PortProcessAction::Kill => (
                                                    NetworkEventKind::ProcessKilled,
                                                    format!(
                                                        "Killed {} (PID: {}) on :{}",
                                                        confirmation.command,
                                                        confirmation.pid,
                                                        confirmation.port
                                                    ),
                                                ),
                                                PortProcessAction::Restart => (
                                                    NetworkEventKind::ProcessKilled,
                                                    format!(
                                                        "Restarted {} (PID: {}) on :{}",
                                                        confirmation.command,
                                                        confirmation.pid,
                                                        confirmation.port
                                                    ),
                                                ),
                                            };
                                            app.push_event(NetworkEvent::new(
                                                kind,
                                                EventSeverity::Info,
                                                message,
                                            ));
                                            request_refresh(
                                                &app,
                                                &mut refresh_task,
                                                &mut last_tick,
                                            );
                                        }
                                        Err(error) => {
                                            let action = match confirmation.action {
                                                PortProcessAction::Kill => "Kill",
                                                PortProcessAction::Restart => "Restart",
                                            };
                                            app.push_event(NetworkEvent::new(
                                                NetworkEventKind::SystemError,
                                                EventSeverity::Error,
                                                format!(
                                                    "{action} failed (PID: {}): {error}",
                                                    confirmation.pid
                                                ),
                                            ));
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // --- Raw viewer mode: intercept all input ---
                    if app.raw_viewer.active {
                        if app.raw_viewer.search_active {
                            match key.code {
                                KeyCode::Esc => {
                                    app.raw_viewer.search_active = false;
                                }
                                KeyCode::Enter => {
                                    app.raw_viewer.search_active = false;
                                    if !app.raw_viewer.search_matches.is_empty() {
                                        app.raw_viewer.scroll =
                                            app.raw_viewer.search_matches[0].line_index as u16;
                                    }
                                }
                                KeyCode::Backspace => {
                                    app.raw_viewer.search_query.pop();
                                    app.update_raw_viewer_search_matches();
                                }
                                KeyCode::Char(c) => {
                                    app.raw_viewer.search_query.push(c);
                                    app.update_raw_viewer_search_matches();
                                }
                                _ => {}
                            }
                        } else {
                            match key.code {
                                KeyCode::Esc
                                | KeyCode::Char('q')
                                | KeyCode::Char('o')
                                | KeyCode::Char('ㅐ') => {
                                    app.raw_viewer.active = false;
                                }
                                KeyCode::Tab => {
                                    app.raw_viewer.selected_index = (app.raw_viewer.selected_index
                                        + 1)
                                        % app.raw_viewer.sources.len();
                                    app.raw_viewer.scroll = 0;
                                    app.update_raw_viewer_search_matches();
                                }
                                KeyCode::BackTab => {
                                    app.raw_viewer.selected_index = (app.raw_viewer.selected_index
                                        + app.raw_viewer.sources.len()
                                        - 1)
                                        % app.raw_viewer.sources.len();
                                    app.raw_viewer.scroll = 0;
                                    app.update_raw_viewer_search_matches();
                                }
                                KeyCode::Char('j') | KeyCode::Down => {
                                    app.raw_viewer.scroll = app.raw_viewer.scroll.saturating_add(1);
                                }
                                KeyCode::Char('k') | KeyCode::Up => {
                                    app.raw_viewer.scroll = app.raw_viewer.scroll.saturating_sub(1);
                                }
                                KeyCode::PageDown => {
                                    app.raw_viewer.scroll =
                                        app.raw_viewer.scroll.saturating_add(15);
                                }
                                KeyCode::PageUp => {
                                    app.raw_viewer.scroll =
                                        app.raw_viewer.scroll.saturating_sub(15);
                                }
                                KeyCode::Home => {
                                    app.raw_viewer.scroll = 0;
                                }
                                KeyCode::End => {
                                    app.raw_viewer.scroll = u16::MAX;
                                }
                                KeyCode::Char('/') => {
                                    app.raw_viewer.search_active = true;
                                    app.raw_viewer.search_query.clear();
                                    app.raw_viewer.search_matches.clear();
                                }
                                KeyCode::Char('n') => {
                                    if !app.raw_viewer.search_matches.is_empty() {
                                        app.raw_viewer.current_match_index =
                                            (app.raw_viewer.current_match_index + 1)
                                                % app.raw_viewer.search_matches.len();
                                        app.raw_viewer.scroll = app.raw_viewer.search_matches
                                            [app.raw_viewer.current_match_index]
                                            .line_index
                                            as u16;
                                    }
                                }
                                KeyCode::Char('N') => {
                                    if !app.raw_viewer.search_matches.is_empty() {
                                        app.raw_viewer.current_match_index =
                                            (app.raw_viewer.current_match_index
                                                + app.raw_viewer.search_matches.len()
                                                - 1)
                                                % app.raw_viewer.search_matches.len();
                                        app.raw_viewer.scroll = app.raw_viewer.search_matches
                                            [app.raw_viewer.current_match_index]
                                            .line_index
                                            as u16;
                                    }
                                }
                                KeyCode::Char('y') => {
                                    if let Some(&src_id) =
                                        app.raw_viewer.sources.get(app.raw_viewer.selected_index)
                                    {
                                        let command = raw_viewer_command_to_copy(&app, src_id);
                                        let _ = lazyifconfig::command::copy_to_clipboard(&command);
                                        app.recent_events.push(NetworkEvent::new(
                                            NetworkEventKind::ActionCopied,
                                            EventSeverity::Info,
                                            format!("Copied command: {command}"),
                                        ));
                                    }
                                }
                                KeyCode::Char('Y') => {
                                    if let Some(&src_id) =
                                        app.raw_viewer.sources.get(app.raw_viewer.selected_index)
                                    {
                                        if let Some(out) = app.command_outputs.get(&src_id) {
                                            let text = format!("{}\n{}", out.stdout, out.stderr);
                                            let _ = lazyifconfig::command::copy_to_clipboard(&text);
                                            app.recent_events.push(NetworkEvent::new(
                                                NetworkEventKind::ActionCopied,
                                                EventSeverity::Info,
                                                format!(
                                                    "Copied raw output for: {}",
                                                    src_id.as_str()
                                                ),
                                            ));
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        continue;
                    }

                    if app.release_notes_viewer.active {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('R') => {
                                app.release_notes_viewer.active = false;
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                app.release_notes_viewer.scroll =
                                    app.release_notes_viewer.scroll.saturating_add(1);
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                app.release_notes_viewer.scroll =
                                    app.release_notes_viewer.scroll.saturating_sub(1);
                            }
                            KeyCode::PageDown => {
                                app.release_notes_viewer.scroll =
                                    app.release_notes_viewer.scroll.saturating_add(12);
                            }
                            KeyCode::PageUp => {
                                app.release_notes_viewer.scroll =
                                    app.release_notes_viewer.scroll.saturating_sub(12);
                            }
                            KeyCode::Home => {
                                app.release_notes_viewer.scroll = 0;
                            }
                            KeyCode::End => {
                                app.release_notes_viewer.scroll = u16::MAX;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if app.route_inspector.destination_input_active {
                        match key.code {
                            KeyCode::Esc => {
                                app.route_inspector.destination_input_active = false;
                            }
                            KeyCode::Enter => {
                                app.route_inspector.destination_input_active = false;
                                run_route_path_lookup(&mut app);
                            }
                            KeyCode::Backspace => {
                                app.route_inspector.destination_input.pop();
                            }
                            KeyCode::Char(c) => {
                                app.route_inspector.destination_input.push(c);
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if app.route_inspector.route_filter_active {
                        match key.code {
                            KeyCode::Esc => {
                                app.route_inspector.route_filter.clear();
                                app.route_inspector.route_filter_active = false;
                                app.update_navigation_items();
                            }
                            KeyCode::Enter => {
                                app.route_inspector.route_filter_active = false;
                            }
                            KeyCode::Backspace => {
                                app.route_inspector.route_filter.pop();
                                app.update_navigation_items();
                                app.selected_index = 0;
                            }
                            KeyCode::Char(c) => {
                                app.route_inspector.route_filter.push(c);
                                app.update_navigation_items();
                                app.selected_index = 0;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // --- Filter mode: intercept all input ---
                    if app.port_filter_active {
                        match key.code {
                            KeyCode::Esc => {
                                app.port_filter.clear();
                                app.port_filter_active = false;
                                app.update_navigation_items();
                            }
                            KeyCode::Enter => {
                                app.port_filter_active = false;
                            }
                            KeyCode::Backspace => {
                                app.port_filter.pop();
                                app.update_navigation_items();
                                app.selected_index = 0;
                            }
                            KeyCode::Char(c) => {
                                app.port_filter.push(c);
                                app.update_navigation_items();
                                app.selected_index = 0;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if app.connection_filter_active {
                        match key.code {
                            KeyCode::Esc => {
                                app.connection_filter.clear();
                                app.connection_filter_active = false;
                                app.update_navigation_items();
                            }
                            KeyCode::Enter => {
                                app.connection_filter_active = false;
                            }
                            KeyCode::Backspace => {
                                app.connection_filter.pop();
                                app.update_navigation_items();
                                app.selected_index = 0;
                            }
                            KeyCode::Char(c) => {
                                app.connection_filter.push(c);
                                app.update_navigation_items();
                                app.selected_index = 0;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if app.view_mode == ViewMode::Tools {
                        if app.tools.input_modal_open {
                            match key.code {
                                KeyCode::Esc => {
                                    app.tools.close_input_modal();
                                }
                                KeyCode::Tab => {
                                    app.tools.select_next_field();
                                }
                                KeyCode::Backspace => {
                                    app.tools.pop_input_char();
                                }
                                KeyCode::Enter => {
                                    app.tools.close_input_modal();
                                    start_selected_tool(&mut app);
                                }
                                KeyCode::Char(c) => {
                                    app.tools.push_input_char(c);
                                }
                                _ => {}
                            }
                            continue;
                        }

                        match key.code {
                            KeyCode::Char('q') | KeyCode::Char('ㅂ') => break,
                            KeyCode::Esc => {
                                app.help_visible = false;
                            }
                            KeyCode::Char('?') => {
                                app.help_visible = !app.help_visible;
                            }
                            KeyCode::Char('i') | KeyCode::Char('ㅑ') => {
                                app.help_visible = false;
                                app.set_view_mode(ViewMode::Interface);
                            }
                            KeyCode::Char('n') | KeyCode::Char('ㅜ') => {
                                app.help_visible = false;
                                app.set_view_mode(ViewMode::Network);
                            }
                            KeyCode::Char('p') | KeyCode::Char('ㅔ') => {
                                app.help_visible = false;
                                app.set_view_mode(ViewMode::Ports);
                            }
                            KeyCode::Char('c') | KeyCode::Char('ㅊ') => {
                                app.help_visible = false;
                                app.set_view_mode(ViewMode::Connections);
                            }
                            KeyCode::Char('g') | KeyCode::Char('ㅎ') => {
                                app.help_visible = false;
                                app.set_view_mode(ViewMode::Routes);
                            }
                            KeyCode::Char('e') | KeyCode::Char('ㄷ') => {
                                app.help_visible = false;
                                app.set_view_mode(ViewMode::Timeline);
                            }
                            KeyCode::Char('h') | KeyCode::Left | KeyCode::Char('ㅗ') => {
                                app.help_visible = false;
                                app.select_previous_view_mode();
                            }
                            KeyCode::Char('l') | KeyCode::Right | KeyCode::Char('ㅣ') => {
                                app.help_visible = false;
                                app.select_next_view_mode();
                            }
                            KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('ㅓ')
                                if !app.tools.editing_input =>
                            {
                                app.tools.select_next_tool();
                            }
                            KeyCode::Char('k') | KeyCode::Up | KeyCode::Char('ㅏ')
                                if !app.tools.editing_input =>
                            {
                                app.tools.select_previous_tool();
                            }
                            KeyCode::Char('/') => {
                                app.help_visible = false;
                                app.tools.open_input_modal();
                            }
                            KeyCode::Tab => {
                                app.tools.select_next_field();
                                app.tools.open_input_modal();
                            }
                            KeyCode::Enter => {
                                app.help_visible = false;
                                app.tools.open_input_modal();
                            }
                            KeyCode::Char('r') | KeyCode::Char('ㄱ') => {
                                app.help_visible = false;
                                start_selected_tool(&mut app);
                            }
                            KeyCode::Char('o') | KeyCode::Char('ㅐ') => {
                                if app.tools.selected_tool_is_dns_lookup() {
                                    app.tools.toggle_dns_raw_output();
                                    app.tools.raw_scroll = 0;
                                }
                            }
                            KeyCode::Char('[') => {
                                if !app.tools.selected_tool_is_dns_lookup()
                                    || app.tools.dns_raw_output_expanded
                                {
                                    app.tools.raw_scroll = app.tools.raw_scroll.saturating_sub(1);
                                }
                            }
                            KeyCode::Char(']')
                                if !app.tools.selected_tool_is_dns_lookup()
                                    || app.tools.dns_raw_output_expanded =>
                            {
                                app.tools.raw_scroll = app.tools.raw_scroll.saturating_add(1);
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // --- Normal mode ---
                    match key.code {
                        KeyCode::Esc => {
                            app.help_visible = false;
                        }
                        KeyCode::Char('?') => {
                            app.help_visible = !app.help_visible;
                        }
                        KeyCode::Char('o') | KeyCode::Char('ㅐ') => {
                            app.help_visible = false;
                            let sources = match app.view_mode {
                                ViewMode::Interface | ViewMode::Network => {
                                    let mut sources = vec![CommandSourceId::Ifconfig];
                                    if app
                                        .command_outputs
                                        .contains_key(&CommandSourceId::InterfaceStats)
                                    {
                                        sources.push(CommandSourceId::InterfaceStats);
                                    }
                                    sources
                                }
                                ViewMode::Connections => vec![CommandSourceId::NetstatConnections],
                                ViewMode::Ports => vec![CommandSourceId::LsofPorts],
                                ViewMode::Routes => routes_raw_sources(&app),
                                ViewMode::Timeline => vec![
                                    CommandSourceId::Ifconfig,
                                    CommandSourceId::NetstatRoutes,
                                    CommandSourceId::DefaultRoute,
                                    CommandSourceId::PublicIp,
                                    CommandSourceId::GitHubRelease,
                                ],
                                ViewMode::Tools => Vec::new(),
                            };
                            if !sources.is_empty() {
                                app.raw_viewer.active = true;
                                app.raw_viewer.sources = sources;
                                app.raw_viewer.selected_index = 0;
                                app.raw_viewer.scroll = 0;
                                app.raw_viewer.search_query.clear();
                                app.raw_viewer.search_active = false;
                                app.raw_viewer.search_matches.clear();
                            }
                        }
                        KeyCode::Char('q') | KeyCode::Char('ㅂ') => break,
                        KeyCode::Char('r') | KeyCode::Char('ㄱ') => {
                            app.help_visible = false;
                            request_refresh(&app, &mut refresh_task, &mut last_tick);
                        }
                        KeyCode::Char('u') | KeyCode::Char('ㅕ') => {
                            app.help_visible = false;
                            start_update_check(&mut app, true);
                        }
                        KeyCode::Char('U') => {
                            app.help_visible = false;
                            start_update_install(&mut app, true);
                        }
                        KeyCode::Char('X') => {
                            if matches!(
                                app.update_status,
                                lazyifconfig::update::UpdateStatus::Updated { .. }
                            ) {
                                restart_requested = true;
                                break;
                            }
                        }
                        KeyCode::Char('R') => {
                            app.help_visible = false;
                            app.release_notes_viewer.active = true;
                            app.release_notes_viewer.scroll = 0;
                        }
                        KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('ㅓ') => {
                            app.select_next();
                        }
                        KeyCode::Char('k') | KeyCode::Up | KeyCode::Char('ㅏ') => {
                            app.select_previous();
                        }
                        KeyCode::Char('l') | KeyCode::Right | KeyCode::Char('ㅣ') => {
                            app.help_visible = false;
                            app.select_next_view_mode();
                            if app.view_mode == ViewMode::Ports
                                && should_refresh_immediately_after_port_navigation()
                            {
                                request_refresh(&app, &mut refresh_task, &mut last_tick);
                            }
                        }
                        KeyCode::Char('h') | KeyCode::Left | KeyCode::Char('ㅗ') => {
                            app.help_visible = false;
                            app.select_previous_view_mode();
                            if app.view_mode == ViewMode::Ports
                                && should_refresh_immediately_after_port_navigation()
                            {
                                request_refresh(&app, &mut refresh_task, &mut last_tick);
                            }
                        }
                        KeyCode::Tab => {
                            if app.view_mode == ViewMode::Routes {
                                app.select_next_route_section();
                            } else if app.view_mode == ViewMode::Connections {
                                app.select_next_connection_details_section();
                            } else if app.view_mode == ViewMode::Ports {
                                app.select_next_port_details_section();
                                if app.port_details_section
                                    == lazyifconfig::app::PortDetailsSection::Detail
                                    && should_refresh_immediately_after_port_navigation()
                                {
                                    request_refresh(&app, &mut refresh_task, &mut last_tick);
                                }
                            }
                        }
                        KeyCode::BackTab => {
                            if app.view_mode == ViewMode::Routes {
                                app.select_previous_route_section();
                            } else if app.view_mode == ViewMode::Connections {
                                app.select_previous_connection_details_section();
                            } else if app.view_mode == ViewMode::Ports {
                                app.select_previous_port_details_section();
                                if app.port_details_section
                                    == lazyifconfig::app::PortDetailsSection::Detail
                                    && should_refresh_immediately_after_port_navigation()
                                {
                                    request_refresh(&app, &mut refresh_task, &mut last_tick);
                                }
                            }
                        }
                        KeyCode::Home => {
                            if app.view_mode == ViewMode::Routes {
                                app.select_route_section_by_index(0);
                            }
                        }
                        KeyCode::End => {
                            if app.view_mode == ViewMode::Routes {
                                app.select_route_section_by_index(3);
                            }
                        }
                        KeyCode::Enter => {
                            if app.view_mode == ViewMode::Routes {
                                app.route_inspector.destination_input_active = true;
                                app.route_inspector.active_section =
                                    RouteInspectorSection::PathViewer;
                            }
                        }
                        KeyCode::Char('1') => {
                            if app.view_mode == ViewMode::Routes {
                                app.select_route_section_by_index(0);
                            }
                        }
                        KeyCode::Char('2') => {
                            if app.view_mode == ViewMode::Routes {
                                app.select_route_section_by_index(1);
                            }
                        }
                        KeyCode::Char('3') => {
                            if app.view_mode == ViewMode::Routes {
                                app.select_route_section_by_index(2);
                            }
                        }
                        KeyCode::Char('4') => {
                            if app.view_mode == ViewMode::Routes {
                                app.select_route_section_by_index(3);
                            }
                        }
                        KeyCode::Char('5') => {
                            if app.view_mode == ViewMode::Routes {
                                app.select_route_section_by_index(3);
                            }
                        }
                        KeyCode::Char('K') => {
                            app.help_visible = false;
                            if app.view_mode == ViewMode::Ports {
                                app.open_selected_port_action_confirmation();
                            }
                        }
                        KeyCode::Char('/') => {
                            app.help_visible = false;
                            match app.view_mode {
                                ViewMode::Ports => app.port_filter_active = true,
                                ViewMode::Connections => app.connection_filter_active = true,
                                ViewMode::Routes => app.route_inspector.route_filter_active = true,
                                _ => {}
                            }
                        }
                        KeyCode::Char('s') | KeyCode::Char('ㄴ') => {
                            app.help_visible = false;
                            if app.view_mode == ViewMode::Ports {
                                app.cycle_port_sort_column();
                            } else if app.view_mode == ViewMode::Connections {
                                app.cycle_connection_sort_column();
                            } else if app.view_mode == ViewMode::Routes {
                                app.cycle_route_sort_column();
                            }
                        }
                        KeyCode::Char('S') => {
                            app.help_visible = false;
                            if app.view_mode == ViewMode::Timeline {
                                match lazyifconfig::command::save_timeline_events_to_file(
                                    &app.recent_events,
                                ) {
                                    Ok(path) => {
                                        app.push_event(NetworkEvent::new(
                                            NetworkEventKind::TimelineExported,
                                            EventSeverity::Info,
                                            format!("Saved timeline to {}", path.display()),
                                        ));
                                    }
                                    Err(error) => {
                                        app.push_event(NetworkEvent::new(
                                            NetworkEventKind::SystemError,
                                            EventSeverity::Error,
                                            format!("Failed to save timeline: {}", error),
                                        ));
                                    }
                                }
                            } else if app.view_mode == ViewMode::Ports {
                                app.toggle_port_sort_direction();
                            } else if app.view_mode == ViewMode::Connections {
                                app.toggle_connection_sort_direction();
                            } else if app.view_mode == ViewMode::Routes {
                                app.toggle_route_sort_direction();
                            }
                        }
                        KeyCode::Char('a') | KeyCode::Char('ㅁ') => {
                            app.help_visible = false;
                            app.show_all = !app.show_all;
                            request_refresh(&app, &mut refresh_task, &mut last_tick);
                        }
                        KeyCode::Char('i') | KeyCode::Char('ㅑ') => {
                            app.help_visible = false;
                            app.set_view_mode(ViewMode::Interface);
                        }
                        KeyCode::Char('n') | KeyCode::Char('ㅜ') => {
                            app.help_visible = false;
                            app.set_view_mode(ViewMode::Network);
                        }
                        KeyCode::Char('p') | KeyCode::Char('ㅔ') => {
                            app.help_visible = false;
                            app.set_view_mode(ViewMode::Ports);
                            if should_refresh_immediately_after_port_navigation() {
                                request_refresh(&app, &mut refresh_task, &mut last_tick);
                            }
                        }
                        KeyCode::Char('e') | KeyCode::Char('ㄷ') => {
                            app.help_visible = false;
                            app.set_view_mode(ViewMode::Timeline);
                        }
                        KeyCode::Char('g') | KeyCode::Char('ㅎ') => {
                            app.help_visible = false;
                            app.set_view_mode(ViewMode::Routes);
                        }
                        KeyCode::Char('t') | KeyCode::Char('ㅅ') => {
                            app.help_visible = false;
                            app.set_view_mode(ViewMode::Tools);
                        }
                        KeyCode::Char('c') | KeyCode::Char('ㅊ') => {
                            app.help_visible = false;
                            if app.view_mode == ViewMode::Connections {
                                if let Some(NavigationItem::Connection { foreign_ip, .. }) =
                                    app.navigation_items.get(app.selected_index)
                                {
                                    if foreign_ip != "*"
                                        && foreign_ip != "::"
                                        && foreign_ip != "0.0.0.0"
                                        && foreign_ip != "*.*"
                                    {
                                        if let Err(e) =
                                            lazyifconfig::command::copy_to_clipboard(foreign_ip)
                                        {
                                            app.recent_events
                                                .push(lazyifconfig::model::NetworkEvent::new(
                                                lazyifconfig::model::NetworkEventKind::SystemError,
                                                lazyifconfig::model::EventSeverity::Error,
                                                format!("Failed to copy IP: {}", e),
                                            ));
                                            if app.recent_events.len() > 100 {
                                                let overflow = app.recent_events.len() - 100;
                                                app.recent_events.drain(0..overflow);
                                            }
                                        } else {
                                            app.recent_events
                                                .push(lazyifconfig::model::NetworkEvent::new(
                                                lazyifconfig::model::NetworkEventKind::ActionCopied,
                                                lazyifconfig::model::EventSeverity::Info,
                                                format!("Copied IP {} to clipboard", foreign_ip),
                                            ));
                                            if app.recent_events.len() > 100 {
                                                let overflow = app.recent_events.len() - 100;
                                                app.recent_events.drain(0..overflow);
                                            }
                                        }
                                    }
                                }
                            } else {
                                app.set_view_mode(ViewMode::Connections);
                            }
                        }
                        KeyCode::Char('w') | KeyCode::Char('ㅈ') => {
                            app.help_visible = false;
                            if app.view_mode == ViewMode::Connections {
                                if let Some(NavigationItem::Connection { foreign_ip, .. }) =
                                    app.navigation_items.get(app.selected_index)
                                {
                                    if foreign_ip != "*"
                                        && foreign_ip != "::"
                                        && foreign_ip != "0.0.0.0"
                                        && foreign_ip != "*.*"
                                    {
                                        let mut should_fetch = false;
                                        if let Ok(lock) = app.whois_cache.lock() {
                                            if !lock.contains_key(foreign_ip)
                                                || lock.get(foreign_ip).map(|s| s.as_str())
                                                    != Some("Loading...")
                                            {
                                                should_fetch = true;
                                            }
                                        }

                                        if should_fetch {
                                            if let Ok(mut lock) = app.whois_cache.lock() {
                                                lock.insert(
                                                    foreign_ip.to_string(),
                                                    "Loading...".to_string(),
                                                );
                                            }

                                            app.recent_events
                                                .push(lazyifconfig::model::NetworkEvent::new(
                                                lazyifconfig::model::NetworkEventKind::ActionWhois,
                                                lazyifconfig::model::EventSeverity::Info,
                                                format!("Starting WHOIS lookup for {}", foreign_ip),
                                            ));
                                            if app.recent_events.len() > 100 {
                                                let overflow = app.recent_events.len() - 100;
                                                app.recent_events.drain(0..overflow);
                                            }

                                            let cache_clone = app.whois_cache.clone();
                                            let ip_clone = foreign_ip.to_string();

                                            tokio::spawn(async move {
                                                let result = match lazyifconfig::command::run_whois(
                                                    &ip_clone,
                                                ) {
                                                    Ok(out) => out,
                                                    Err(e) => format!("Error running whois: {}", e),
                                                };
                                                if let Ok(mut lock) = cache_clone.lock() {
                                                    lock.insert(ip_clone, result);
                                                }
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Char('[') => {
                            app.help_visible = false;
                            app.scroll_details_up();
                        }
                        KeyCode::Char(']') => {
                            app.help_visible = false;
                            app.scroll_details_down();
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        if last_tick.elapsed() >= tick_rate {
            request_refresh(&app, &mut refresh_task, &mut last_tick);
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if restart_requested {
        let exe = std::env::current_exe()?;
        let args = std::env::args_os().skip(1).collect::<Vec<_>>();
        std::process::Command::new(exe).args(args).spawn()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazyifconfig::model::{
        InterfaceStatus, InterfaceType, NetworkInterface, NetworkKind, NetworkSnapshot,
    };

    #[test]
    fn windows_does_not_refresh_synchronously_on_port_navigation() {
        assert_eq!(
            should_refresh_immediately_after_port_navigation(),
            !cfg!(target_os = "windows")
        );
    }

    #[test]
    fn applying_refresh_result_preserves_current_view_and_selected_interface() {
        let mut app = App::default();
        app.replace_snapshot(test_snapshot(vec![
            test_interface("Wi-Fi"),
            test_interface("VPN"),
        ]));
        app.selected_index = 1;

        let mut refreshed = app.clone();
        refreshed.replace_snapshot(test_snapshot(vec![
            test_interface("Loopback"),
            test_interface("VPN"),
            test_interface("Wi-Fi"),
        ]));
        refreshed.set_view_mode(ViewMode::Tools);

        apply_refresh_result(&mut app, refreshed, 0);

        assert_eq!(app.view_mode, ViewMode::Interface);
        assert_eq!(app.selected_interface_name(), Some("VPN"));
    }

    #[test]
    fn applying_refresh_result_appends_new_refresh_events_without_dropping_local_events() {
        let mut app = App::default();
        app.push_event(NetworkEvent::new(
            NetworkEventKind::ActionCopied,
            EventSeverity::Info,
            "local event".to_string(),
        ));
        let base_event_count = app.recent_events.len();

        let mut refreshed = app.clone();
        refreshed.push_event(NetworkEvent::new(
            NetworkEventKind::PublicIpChanged,
            EventSeverity::Info,
            "refresh event".to_string(),
        ));

        app.push_event(NetworkEvent::new(
            NetworkEventKind::ActionCopied,
            EventSeverity::Info,
            "later local event".to_string(),
        ));

        apply_refresh_result(&mut app, refreshed, base_event_count);

        let messages = app
            .recent_events
            .iter()
            .map(|event| event.message.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            messages,
            vec!["local event", "later local event", "refresh event"]
        );
    }

    #[test]
    fn applying_refresh_result_keeps_non_refresh_command_outputs() {
        let mut app = App::default();
        app.command_outputs.insert(
            CommandSourceId::RoutePath,
            test_command_output("fresh route path output"),
        );

        let mut refreshed = app.clone();
        refreshed.command_outputs.insert(
            CommandSourceId::RoutePath,
            test_command_output("stale route path output"),
        );
        refreshed.command_outputs.insert(
            CommandSourceId::Ifconfig,
            test_command_output("fresh interface output"),
        );

        apply_refresh_result(&mut app, refreshed, 0);

        assert_eq!(
            app.command_outputs
                .get(&CommandSourceId::RoutePath)
                .map(|output| output.stdout.as_str()),
            Some("fresh route path output")
        );
        assert_eq!(
            app.command_outputs
                .get(&CommandSourceId::Ifconfig)
                .map(|output| output.stdout.as_str()),
            Some("fresh interface output")
        );
    }

    fn test_snapshot(interfaces: Vec<NetworkInterface>) -> NetworkSnapshot {
        NetworkSnapshot {
            interfaces,
            connections: Vec::new(),
            listening_ports: Vec::new(),
            routes: Vec::new(),
            captured_at_secs: 1,
        }
    }

    fn test_interface(name: &str) -> NetworkInterface {
        NetworkInterface {
            name: name.to_string(),
            network_kind: NetworkKind::Lan,
            interface_type: InterfaceType::WifiOrEthernet,
            status: InterfaceStatus::Up,
            ipv4: Vec::new(),
            ipv6: Vec::new(),
            mac_address: None,
            mtu: None,
            stats: None,
        }
    }

    fn test_command_output(stdout: &str) -> lazyifconfig::model::CommandOutput {
        lazyifconfig::model::CommandOutput {
            command: "test".to_string(),
            stdout: stdout.to_string(),
            stderr: String::new(),
            executed_at: std::time::SystemTime::now(),
            exit_code: Some(0),
        }
    }
}
