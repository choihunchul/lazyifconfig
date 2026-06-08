use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use lazyifconfig::app::{App, ViewMode, NavigationItem};
use lazyifconfig::command::{run_ifconfig, run_netstat, run_netstat_an, run_netstat_ib};
use lazyifconfig::collector::interface::{parse_interfaces, merge_gateways};
use lazyifconfig::collector::stats::merge_stats;
use lazyifconfig::collector::connections::parse_connections;
use lazyifconfig::model::NetworkSnapshot;

pub fn tick_update(app: &mut App) -> Result<(), String> {
    let raw_out = run_ifconfig(app.show_all)?;
    let mut parsed = parse_interfaces(&raw_out);
    
    if let Ok(netstat_out) = run_netstat() {
        merge_gateways(&mut parsed, &netstat_out);
    }
    
    let stats_out = run_netstat_ib().unwrap_or_else(|_| raw_out.clone());
    let merged = merge_stats(&stats_out, parsed);

    let connections = if let Ok(netstat_an_out) = run_netstat_an() {
        parse_connections(&netstat_an_out)
    } else {
        Vec::new()
    };
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    app.replace_snapshot(NetworkSnapshot {
        interfaces: merged,
        connections,
        captured_at_secs: now,
    });
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::default();
    let _ = tick_update(&mut app);

    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_secs(2);

    loop {
        terminal.draw(|f| lazyifconfig::ui::draw(f, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('ㅂ') => break,
                    KeyCode::Char('r') | KeyCode::Char('ㄱ') => {
                        let _ = tick_update(&mut app);
                        last_tick = std::time::Instant::now();
                    }
                    KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('ㅓ') => {
                        app.select_next();
                    }
                    KeyCode::Char('k') | KeyCode::Up | KeyCode::Char('ㅏ') => {
                        app.select_previous();
                    }
                    KeyCode::Char('a') | KeyCode::Char('ㅁ') => {
                        app.show_all = !app.show_all;
                        let _ = tick_update(&mut app);
                        last_tick = std::time::Instant::now();
                    }
                    KeyCode::Char('i') | KeyCode::Char('ㅑ') => {
                        app.set_view_mode(ViewMode::Interface);
                    }
                    KeyCode::Char('n') | KeyCode::Char('ㅜ') => {
                        app.set_view_mode(ViewMode::Network);
                    }
                    KeyCode::Char('c') | KeyCode::Char('ㅊ') => {
                        if app.view_mode == ViewMode::Connections {
                            if let Some(NavigationItem::Connection { foreign, .. }) = app.navigation_items.get(app.selected_index) {
                                let foreign_ip = if let Some(pos) = foreign.rfind(':') {
                                    &foreign[..pos]
                                } else {
                                    foreign.as_str()
                                };
                                if foreign_ip != "*" && foreign_ip != "::" && foreign_ip != "0.0.0.0" && foreign_ip != "*.*" {
                                    if let Err(e) = lazyifconfig::command::copy_to_clipboard(foreign_ip) {
                                        let now = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .map(|d| d.as_secs())
                                            .unwrap_or(0);
                                        app.recent_events.push(lazyifconfig::model::NetworkEvent::new(
                                            format!("Failed to copy IP: {}", e),
                                            now,
                                        ));
                                        if app.recent_events.len() > 50 {
                                            let overflow = app.recent_events.len() - 50;
                                            app.recent_events.drain(0..overflow);
                                        }
                                    } else {
                                        let now = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .map(|d| d.as_secs())
                                            .unwrap_or(0);
                                        app.recent_events.push(lazyifconfig::model::NetworkEvent::new(
                                            format!("Copied IP {} to clipboard", foreign_ip),
                                            now,
                                        ));
                                        if app.recent_events.len() > 50 {
                                            let overflow = app.recent_events.len() - 50;
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
                        if app.view_mode == ViewMode::Connections {
                            if let Some(NavigationItem::Connection { foreign, .. }) = app.navigation_items.get(app.selected_index) {
                                let foreign_ip = if let Some(pos) = foreign.rfind(':') {
                                    &foreign[..pos]
                                } else {
                                    foreign.as_str()
                                };
                                if foreign_ip != "*" && foreign_ip != "::" && foreign_ip != "0.0.0.0" && foreign_ip != "*.*" {
                                    let mut should_fetch = false;
                                    if let Ok(lock) = app.whois_cache.lock() {
                                        if !lock.contains_key(foreign_ip) || lock.get(foreign_ip).map(|s| s.as_str()) != Some("Loading...") {
                                            should_fetch = true;
                                        }
                                    }
                                    
                                    if should_fetch {
                                        if let Ok(mut lock) = app.whois_cache.lock() {
                                            lock.insert(foreign_ip.to_string(), "Loading...".to_string());
                                        }
                                        
                                        let now = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .map(|d| d.as_secs())
                                            .unwrap_or(0);
                                        app.recent_events.push(lazyifconfig::model::NetworkEvent::new(
                                            format!("Starting WHOIS lookup for {}", foreign_ip),
                                            now,
                                        ));
                                        if app.recent_events.len() > 50 {
                                            let overflow = app.recent_events.len() - 50;
                                            app.recent_events.drain(0..overflow);
                                        }
                                        
                                        let cache_clone = app.whois_cache.clone();
                                        let ip_clone = foreign_ip.to_string();
                                        
                                        tokio::spawn(async move {
                                            let result = match lazyifconfig::command::run_whois(&ip_clone) {
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
                        app.scroll_details_up();
                    }
                    KeyCode::Char(']') => {
                        app.scroll_details_down();
                    }
                    _ => {}
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_update() {
        let mut app = App::default();
        let res = tick_update(&mut app);
        assert!(res.is_ok());
        assert!(app.current_snapshot.is_some());
    }
}
