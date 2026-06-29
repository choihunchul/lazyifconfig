#![cfg(target_os = "windows")]

use lazyifconfig::app::{App, ViewMode};
use lazyifconfig::collector::windows::collect_windows_interface_stats;
use lazyifconfig::model::CommandSourceId;
use lazyifconfig::runtime::refresh::tick_update;

#[tokio::test]
async fn runtime_tick_collects_visible_values_on_local_windows() {
    let mut app = App::default();
    app.set_view_mode(ViewMode::Ports);
    tick_update(&mut app).expect("tick update should collect command output");

    let snapshot = app.current_snapshot.expect("snapshot should be populated");
    assert!(
        !snapshot.interfaces.is_empty(),
        "interface view should have parsed values"
    );
    assert!(
        !snapshot.listening_ports.is_empty(),
        "port view should have parsed values"
    );
    assert!(
        !snapshot.routes.is_empty(),
        "route view should have parsed values"
    );
    assert!(
        !snapshot.connections.is_empty(),
        "connection view should have parsed values"
    );
}

#[test]
fn native_interface_stats_are_available_without_powershell() {
    let stats = collect_windows_interface_stats().expect("native interface stats");

    assert!(
        !stats.is_empty(),
        "native interface stats should not require PowerShell CIM access"
    );
}

#[test]
fn interface_stats_source_label_uses_native_windows_api() {
    assert_eq!(CommandSourceId::InterfaceStats.as_str(), "GetIfTable2");
}
