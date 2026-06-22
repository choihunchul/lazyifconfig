#![cfg(target_os = "windows")]

use lazyifconfig::app::{App, ViewMode};
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
