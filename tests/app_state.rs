use lazyifconfig::app::App;
use lazyifconfig::model::{
    InterfaceAddress, InterfaceStats, InterfaceStatus, InterfaceType, NetworkEvent,
    NetworkInterface, NetworkSnapshot,
};

#[test]
fn snapshot_can_hold_interfaces_and_events() {
    let interface = NetworkInterface {
        name: "en0".to_string(),
        network_kind: lazyifconfig::model::NetworkKind::Unknown,
        interface_type: InterfaceType::WifiOrEthernet,
        status: InterfaceStatus::Up,
        ipv4: vec![InterfaceAddress::new("192.168.0.10")],
        ipv6: vec![],
        mac_address: Some("aa:bb:cc:dd:ee:ff".to_string()),
        mtu: Some(1500),
        stats: Some(InterfaceStats {
            rx_bytes: 100,
            tx_bytes: 50,
            rx_packets: 10,
            tx_packets: 5,
        }),
    };

    let event = NetworkEvent::new("en0 appeared".to_string(), 10);

    let snapshot = NetworkSnapshot {
        interfaces: vec![interface],
        captured_at_secs: 10,
    };

    assert_eq!(snapshot.interfaces.len(), 1);
    assert_eq!(snapshot.interfaces[0].stats.as_ref().unwrap().rx_bytes, 100);
    assert_eq!(event.message, "en0 appeared");
}

#[test]
fn replace_snapshot_preserves_selection_by_interface_name() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![
            interface_with_stats("en0", Some("192.168.0.10"), Some((100, 50))),
            interface_with_stats("utun0", None, Some((20, 10))),
        ],
    ));
    app.selected_index = 1;

    app.replace_snapshot(snapshot_with_interfaces(
        20,
        vec![
            interface_with_stats("lo0", Some("127.0.0.1"), None),
            interface_with_stats("utun0", None, Some((40, 30))),
            interface_with_stats("en0", Some("192.168.0.10"), Some((200, 80))),
        ],
    ));

    assert_eq!(app.selected_interface_name(), Some("utun0"));
    assert_eq!(app.selected_index, 1);
}

#[test]
fn selected_rates_are_computed_from_consecutive_snapshots() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![interface_with_stats("en0", Some("192.168.0.10"), Some((1_000, 400)))],
    ));
    app.replace_snapshot(snapshot_with_interfaces(
        15,
        vec![interface_with_stats("en0", Some("192.168.0.10"), Some((1_600, 700)))],
    ));

    assert_eq!(app.selected_rates(), Some((120, 60)));
}

#[test]
fn replace_snapshot_does_not_emit_events_for_first_snapshot() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![interface_with_stats("en0", Some("192.168.0.10"), Some((100, 50)))],
    ));

    assert!(app.recent_events.is_empty());
}

#[test]
fn replace_snapshot_emits_appearance_event() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(10, vec![]));
    app.replace_snapshot(snapshot_with_interfaces(
        20,
        vec![interface_with_stats("en0", Some("192.168.0.10"), Some((100, 50)))],
    ));

    assert_eq!(app.recent_events.len(), 1);
    assert_eq!(app.recent_events[0].captured_at_secs, 20);
    assert_eq!(app.recent_events[0].message, "en0 appeared");
}

#[test]
fn replace_snapshot_emits_disappearance_event() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![interface_with_stats("en0", Some("192.168.0.10"), Some((100, 50)))],
    ));
    app.replace_snapshot(snapshot_with_interfaces(20, vec![]));

    assert_eq!(app.recent_events.len(), 1);
    assert_eq!(app.recent_events[0].captured_at_secs, 20);
    assert_eq!(app.recent_events[0].message, "en0 disappeared");
}

#[test]
fn replace_snapshot_emits_status_change_event() {
    let mut app = App::default();
    app.show_all = true;

    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![interface_with_status(
            "en0",
            InterfaceStatus::Up,
            Some("192.168.0.10"),
            Some((100, 50)),
        )],
    ));
    app.replace_snapshot(snapshot_with_interfaces(
        20,
        vec![interface_with_status(
            "en0",
            InterfaceStatus::Down,
            Some("192.168.0.10"),
            Some((200, 150)),
        )],
    ));

    assert_eq!(app.recent_events.len(), 1);
    assert_eq!(app.recent_events[0].captured_at_secs, 20);
    assert_eq!(app.recent_events[0].message, "en0 status changed: up -> down");
}

#[test]
fn replace_snapshot_emits_ipv4_change_event() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![interface_with_stats("en0", Some("192.168.0.10"), Some((100, 50)))],
    ));
    app.replace_snapshot(snapshot_with_interfaces(
        20,
        vec![interface_with_stats("en0", Some("192.168.0.11"), Some((200, 150)))],
    ));

    assert_eq!(app.recent_events.len(), 1);
    assert_eq!(app.recent_events[0].captured_at_secs, 20);
    assert_eq!(
        app.recent_events[0].message,
        "en0 IPv4 changed: 192.168.0.10 -> 192.168.0.11"
    );
}

#[test]
fn replace_snapshot_keeps_only_most_recent_fifty_events() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        0,
        vec![interface_with_stats("en0", Some("192.168.0.0"), Some((0, 0)))],
    ));

    for idx in 1..=55 {
        app.replace_snapshot(snapshot_with_interfaces(
            idx,
            vec![interface_with_stats(
                "en0",
                Some(&format!("192.168.0.{idx}")),
                Some((idx * 100, idx * 50)),
            )],
        ));
    }

    assert_eq!(app.recent_events.len(), 50);
    assert_eq!(
        app.recent_events.first().map(|event| event.message.as_str()),
        Some("en0 IPv4 changed: 192.168.0.5 -> 192.168.0.6")
    );
    assert_eq!(
        app.recent_events
            .first()
            .map(|event| event.captured_at_secs),
        Some(6)
    );
    assert_eq!(
        app.recent_events.last().map(|event| event.message.as_str()),
        Some("en0 IPv4 changed: 192.168.0.54 -> 192.168.0.55")
    );
}

fn snapshot_with_interfaces(captured_at_secs: u64, interfaces: Vec<NetworkInterface>) -> NetworkSnapshot {
    NetworkSnapshot {
        interfaces,
        captured_at_secs,
    }
}

fn interface_with_stats(
    name: &str,
    ipv4: Option<&str>,
    stats: Option<(u64, u64)>,
) -> NetworkInterface {
    interface_with_status(name, InterfaceStatus::Up, ipv4, stats)
}

fn interface_with_status(
    name: &str,
    status: InterfaceStatus,
    ipv4: Option<&str>,
    stats: Option<(u64, u64)>,
) -> NetworkInterface {
    NetworkInterface {
        name: name.to_string(),
        network_kind: lazyifconfig::model::NetworkKind::Unknown,
        interface_type: InterfaceType::WifiOrEthernet,
        status,
        ipv4: ipv4.into_iter().map(InterfaceAddress::new).collect(),
        ipv6: vec![],
        mac_address: None,
        mtu: Some(1500),
        stats: stats.map(|(rx_bytes, tx_bytes)| InterfaceStats {
            rx_bytes,
            tx_bytes,
            rx_packets: 0,
            tx_packets: 0,
        }),
    }
}

#[test]
fn test_app_navigation() {
    let mut app = App::default();
    app.replace_snapshot(NetworkSnapshot {
        interfaces: vec![
            interface_with_stats("lo0", None, None),
            interface_with_stats("en0", None, None),
            interface_with_stats("utun0", None, None),
        ],
        captured_at_secs: 10,
    });
    assert_eq!(app.selected_index, 0);

    app.select_next();
    assert_eq!(app.selected_index, 1);

    app.select_next();
    assert_eq!(app.selected_index, 2);

    app.select_next();
    assert_eq!(app.selected_index, 0);

    app.select_previous();
    assert_eq!(app.selected_index, 2);
}

#[test]
fn test_app_network_view_grouping() {
    let mut app = App::default();
    let en0 = NetworkInterface {
        name: "en0".to_string(),
        network_kind: lazyifconfig::model::NetworkKind::Lan,
        interface_type: lazyifconfig::model::InterfaceType::WifiOrEthernet,
        status: lazyifconfig::model::InterfaceStatus::Up,
        ipv4: vec![lazyifconfig::model::InterfaceAddress {
            value: "192.168.0.15".to_string(),
            prefix_len: Some(24),
            gateway: None,
        }],
        ipv6: vec![],
        mac_address: None,
        mtu: None,
        stats: None,
    };
    let lo0 = NetworkInterface {
        name: "lo0".to_string(),
        network_kind: lazyifconfig::model::NetworkKind::Loopback,
        interface_type: lazyifconfig::model::InterfaceType::Loopback,
        status: lazyifconfig::model::InterfaceStatus::Up,
        ipv4: vec![lazyifconfig::model::InterfaceAddress {
            value: "127.0.0.1".to_string(),
            prefix_len: Some(8),
            gateway: None,
        }],
        ipv6: vec![],
        mac_address: None,
        mtu: None,
        stats: None,
    };

    app.replace_snapshot(lazyifconfig::model::NetworkSnapshot {
        interfaces: vec![en0, lo0],
        captured_at_secs: 100,
    });

    // 기본 뷰 모드는 Interface
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Interface);

    // 네트워크 뷰로 전환
    app.set_view_mode(lazyifconfig::app::ViewMode::Network);
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Network);

    // navigation_items 검증: SubnetHeader(127.0.0.0/8) -> lo0 -> SubnetHeader(192.168.0.0/24) -> en0
    assert!(matches!(app.navigation_items[0], lazyifconfig::app::NavigationItem::SubnetHeader(_)));
    assert!(matches!(app.navigation_items[1], lazyifconfig::app::NavigationItem::Interface { .. }));
}

#[test]
fn test_traffic_history_bounding_and_cleanup() {
    let mut app = App::default();

    for idx in 0..=50 {
        let stats = Some(InterfaceStats {
            rx_bytes: idx as u64 * 100,
            tx_bytes: idx as u64 * 50,
            rx_packets: 0,
            tx_packets: 0,
        });

        let interface = NetworkInterface {
            name: "en0".to_string(),
            network_kind: lazyifconfig::model::NetworkKind::Lan,
            interface_type: InterfaceType::WifiOrEthernet,
            status: InterfaceStatus::Up,
            ipv4: vec![],
            ipv6: vec![],
            mac_address: None,
            mtu: None,
            stats,
        };

        app.replace_snapshot(NetworkSnapshot {
            interfaces: vec![interface],
            captured_at_secs: idx as u64,
        });
    }

    let history = app.traffic_history.get("en0").unwrap();
    assert_eq!(history.rx_rates.len(), 40);
    assert_eq!(history.tx_rates.len(), 40);
    assert_eq!(history.rx_rates[0], 100);
    assert_eq!(history.tx_rates[0], 50);

    app.replace_snapshot(NetworkSnapshot {
        interfaces: vec![],
        captured_at_secs: 100,
    });
    assert!(app.traffic_history.get("en0").is_none());
}

