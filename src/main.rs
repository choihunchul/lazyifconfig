use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lazyifconfig::app::{App, NavigationItem, ViewMode};
use lazyifconfig::command::run_kill;
use lazyifconfig::model::{
    CommandSourceId, EventSeverity, NetworkEvent, NetworkEventKind, RouteInspectorSection,
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
    let mut profile_override: Option<String> = None;
    let mut remaining_args = Vec::new();
    let mut iter = cli_args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--profile" {
            if let Some(value) = iter.next() {
                profile_override = Some(value);
            }
        } else {
            remaining_args.push(arg);
        }
    }
    let cli_args = remaining_args;

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
    if let Some(profile_name) = profile_override {
        app.active_profile_name = profile_name;
    }
    let _ = tick_update(&mut app);

    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_secs(2);

    loop {
        app.drain_pending_tool_results();
        terminal.draw(|f| lazyifconfig::ui::draw(f, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::from_secs(0));

        if event::poll(timeout)? {
            match event::read()? {
                Event::Paste(text) => {
                    if app.view_mode == ViewMode::Tools && app.tools.input_modal_open {
                        app.tools.push_input_text(&text);
                    }
                    continue;
                }
                Event::Key(key) => {
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
                                    vec![CommandSourceId::Ifconfig]
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
                            let _ = tick_update(&mut app);
                            last_tick = std::time::Instant::now();
                        }
                        KeyCode::Char('u') | KeyCode::Char('ㅕ') => {
                            app.help_visible = false;
                            start_update_check(&mut app, true);
                        }
                        KeyCode::Char('U') => {
                            app.help_visible = false;
                            start_update_install(&mut app, true);
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
                        }
                        KeyCode::Char('h') | KeyCode::Left | KeyCode::Char('ㅗ') => {
                            app.help_visible = false;
                            app.select_previous_view_mode();
                        }
                        KeyCode::Tab => {
                            if app.view_mode == ViewMode::Routes {
                                app.select_next_route_section();
                            } else if app.view_mode == ViewMode::Ports {
                                app.select_next_port_details_section();
                            } else if app.view_mode == ViewMode::Connections {
                                app.select_next_connection_details_section();
                            }
                        }
                        KeyCode::BackTab => {
                            if app.view_mode == ViewMode::Routes {
                                app.select_previous_route_section();
                            } else if app.view_mode == ViewMode::Ports {
                                app.select_previous_port_details_section();
                            } else if app.view_mode == ViewMode::Connections {
                                app.select_previous_connection_details_section();
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
                                // Kill the selected process
                                if let Some(NavigationItem::ListeningPort {
                                    pid,
                                    command,
                                    port,
                                    ..
                                }) = app.navigation_items.get(app.selected_index)
                                {
                                    let pid = pid.clone();
                                    let command = command.clone();
                                    let port = port.clone();
                                    match run_kill(&pid) {
                                        Ok(()) => {
                                            app.recent_events
                                            .push(lazyifconfig::model::NetworkEvent::new(
                                            lazyifconfig::model::NetworkEventKind::ProcessKilled,
                                            lazyifconfig::model::EventSeverity::Info,
                                            format!(
                                                "Killed {} (PID: {}) on :{}",
                                                command, pid, port
                                            ),
                                        ));
                                            let _ = tick_update(&mut app);
                                            last_tick = std::time::Instant::now();
                                        }
                                        Err(e) => {
                                            app.recent_events
                                                .push(lazyifconfig::model::NetworkEvent::new(
                                                lazyifconfig::model::NetworkEventKind::SystemError,
                                                lazyifconfig::model::EventSeverity::Error,
                                                format!("Kill failed (PID: {}): {}", pid, e),
                                            ));
                                        }
                                    }
                                    if app.recent_events.len() > 100 {
                                        let overflow = app.recent_events.len() - 100;
                                        app.recent_events.drain(0..overflow);
                                    }
                                }
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
                            let _ = tick_update(&mut app);
                            last_tick = std::time::Instant::now();
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
            let _ = tick_update(&mut app);
            last_tick = std::time::Instant::now();
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
