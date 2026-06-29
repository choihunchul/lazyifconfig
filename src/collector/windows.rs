use crate::model::{
    ActiveConnection, InterfaceAddress, InterfaceStats, InterfaceStatus, InterfaceType,
    ListeningPort, NetworkInterface, NetworkKind, RouteEntry, RouteFamily,
};
use serde_json::Value;

pub fn parse_powershell_interfaces(input: &str) -> Vec<NetworkInterface> {
    json_items(input)
        .into_iter()
        .filter_map(|item| {
            let item = &item;
            let name = string_field(item, "InterfaceAlias")
                .or_else(|| string_field(item, "InterfaceDescription"))?;
            let adapter = item.get("NetAdapter").unwrap_or(&Value::Null);
            let status = match string_field(adapter, "Status")
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                "up" => InterfaceStatus::Up,
                _ => InterfaceStatus::Down,
            };
            let mac_address = string_field(adapter, "MacAddress")
                .or_else(|| string_field(item, "MacAddress"))
                .map(|mac| mac.replace('-', ":").to_lowercase());
            let mtu = u64_field(adapter, "MtuSize")
                .or_else(|| u64_field(item, "NlMtu"))
                .and_then(|mtu| u32::try_from(mtu).ok());

            let mut ipv4 = parse_interface_addresses(item.get("IPv4Address"));
            let mut ipv6 = parse_interface_addresses(item.get("IPv6Address"));
            attach_gateways(&mut ipv4, item.get("IPv4DefaultGateway"));
            attach_gateways(&mut ipv6, item.get("IPv6DefaultGateway"));

            let network_kind = classify_windows_interface(&name, &ipv4, &ipv6);
            Some(NetworkInterface {
                interface_type: infer_windows_interface_type(&name),
                network_kind,
                status,
                mtu,
                name,
                ipv4,
                ipv6,
                mac_address,
                stats: None,
            })
        })
        .collect()
}

pub fn parse_powershell_routes(input: &str) -> Vec<RouteEntry> {
    json_items(input)
        .into_iter()
        .filter_map(|item| {
            let item = &item;
            let destination = string_field(item, "DestinationPrefix")?;
            let gateway = string_field(item, "NextHop")
                .filter(|value| !value.is_empty() && value != "0.0.0.0" && value != "::")
                .unwrap_or_else(|| "link".to_string());
            let interface = string_field(item, "InterfaceAlias")
                .or_else(|| string_field(item, "InterfaceIndex"))
                .unwrap_or_else(|| "-".to_string());
            let mut route = RouteEntry::new(
                if destination == "0.0.0.0/0" || destination == "::/0" {
                    "default".to_string()
                } else {
                    destination
                },
                gateway,
                interface,
            );
            route.metric = u64_field(item, "RouteMetric").and_then(|metric| metric.try_into().ok());
            route.protocol = string_field(item, "Protocol");
            route.family = match string_field(item, "AddressFamily")
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                "ipv4" | "2" => RouteFamily::Ipv4,
                "ipv6" | "23" => RouteFamily::Ipv6,
                _ => RouteFamily::Unknown,
            };
            Some(route)
        })
        .collect()
}

pub fn parse_powershell_listening_ports(input: &str) -> Vec<ListeningPort> {
    json_items(input)
        .into_iter()
        .filter_map(|item| {
            let item = &item;
            let pid = string_field(item, "OwningProcess")?;
            Some(ListeningPort {
                proto: "tcp".to_string(),
                local_ip: string_field(item, "LocalAddress").unwrap_or_else(|| "*".to_string()),
                local_port: string_field(item, "LocalPort").unwrap_or_else(|| "*".to_string()),
                pid: pid.clone(),
                command: format!("pid:{pid}"),
                user: "-".to_string(),
                process: None,
            })
        })
        .collect()
}

pub fn parse_powershell_connections(input: &str) -> Vec<ActiveConnection> {
    json_items(input)
        .into_iter()
        .filter_map(|item| {
            let item = &item;
            Some(ActiveConnection {
                proto: "tcp".to_string(),
                local_ip: string_field(item, "LocalAddress")?,
                local_port: string_field(item, "LocalPort")?,
                foreign_ip: string_field(item, "RemoteAddress")?,
                foreign_port: string_field(item, "RemotePort")?,
                state: string_field(item, "State"),
            })
        })
        .collect()
}

pub fn merge_powershell_interface_stats(
    input: &str,
    interfaces: Vec<NetworkInterface>,
) -> Vec<NetworkInterface> {
    let stats = parse_powershell_interface_stats(input);
    merge_windows_interface_stats(interfaces, &stats)
}

pub fn merge_windows_interface_stats(
    mut interfaces: Vec<NetworkInterface>,
    stats: &[WindowsInterfaceStats],
) -> Vec<NetworkInterface> {
    for interface in &mut interfaces {
        if let Some(stat) = stats.iter().find(|stat| stat.matches_name(&interface.name)) {
            interface.stats = Some(stat.stats.clone());
        }
    }
    interfaces
}

fn parse_powershell_interface_stats(input: &str) -> Vec<WindowsInterfaceStats> {
    json_items(input)
        .into_iter()
        .filter_map(|item| {
            let item = &item;
            let name = string_field(item, "Name")
                .or_else(|| string_field(item, "InterfaceAlias"))
                .or_else(|| string_field(item, "InterfaceDescription"))?;
            Some(WindowsInterfaceStats {
                alias: name,
                description: string_field(item, "InterfaceDescription").unwrap_or_default(),
                stats: InterfaceStats {
                    rx_bytes: u64_field(item, "ReceivedBytes").unwrap_or(0),
                    tx_bytes: u64_field(item, "SentBytes").unwrap_or(0),
                    rx_packets: u64_field(item, "ReceivedUnicastPackets")
                        .or_else(|| u64_field(item, "ReceivedPackets"))
                        .unwrap_or(0),
                    tx_packets: u64_field(item, "SentUnicastPackets")
                        .or_else(|| u64_field(item, "SentPackets"))
                        .unwrap_or(0),
                },
            })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsInterfaceStats {
    pub alias: String,
    pub description: String,
    pub stats: InterfaceStats,
}

impl WindowsInterfaceStats {
    fn matches_name(&self, name: &str) -> bool {
        self.alias == name || self.description == name
    }
}

#[cfg(target_os = "windows")]
pub fn collect_windows_interface_stats() -> Result<Vec<WindowsInterfaceStats>, String> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GetIfTable2, MIB_IF_TABLE2,
    };

    struct MibTable(*mut MIB_IF_TABLE2);

    impl Drop for MibTable {
        fn drop(&mut self) {
            unsafe {
                FreeMibTable(self.0.cast());
            }
        }
    }

    let mut table = std::ptr::null_mut();
    let error = unsafe { GetIfTable2(&mut table) };
    if error != 0 {
        return Err(format!("GetIfTable2 failed with Windows error {error}"));
    }
    if table.is_null() {
        return Err("GetIfTable2 returned a null table".to_string());
    }

    let table = MibTable(table);
    let table_ref = unsafe { &*table.0 };
    let rows = unsafe {
        std::slice::from_raw_parts(table_ref.Table.as_ptr(), table_ref.NumEntries as usize)
    };

    Ok(rows
        .iter()
        .map(|row| WindowsInterfaceStats {
            alias: wide_string(&row.Alias),
            description: wide_string(&row.Description),
            stats: InterfaceStats {
                rx_bytes: row.InOctets,
                tx_bytes: row.OutOctets,
                rx_packets: row.InUcastPkts.saturating_add(row.InNUcastPkts),
                tx_packets: row.OutUcastPkts.saturating_add(row.OutNUcastPkts),
            },
        })
        .filter(|stat| !stat.alias.is_empty() || !stat.description.is_empty())
        .collect())
}

#[cfg(not(target_os = "windows"))]
pub fn collect_windows_interface_stats() -> Result<Vec<WindowsInterfaceStats>, String> {
    Err("Windows interface stats are available only on Windows".to_string())
}

fn wide_string(value: &[u16]) -> String {
    let end = value.iter().position(|ch| *ch == 0).unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end]).trim().to_string()
}

fn parse_interface_addresses(value: Option<&Value>) -> Vec<InterfaceAddress> {
    value
        .map(value_items)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| {
            let address = string_field(item, "IPAddress")?;
            Some(InterfaceAddress {
                value: clean_scope_id(&address),
                prefix_len: u64_field(item, "PrefixLength")
                    .and_then(|prefix| prefix.try_into().ok()),
                gateway: None,
            })
        })
        .collect()
}

fn attach_gateways(addresses: &mut [InterfaceAddress], gateways: Option<&Value>) {
    let Some(gateway) = gateways
        .map(value_items)
        .unwrap_or_default()
        .into_iter()
        .find_map(|item| string_field(item, "NextHop"))
    else {
        return;
    };

    for address in addresses {
        if address.gateway.is_none() {
            address.gateway = Some(gateway.clone());
        }
    }
}

fn json_items(input: &str) -> Vec<Value> {
    let Ok(value) = serde_json::from_str::<Value>(input) else {
        return Vec::new();
    };
    match value {
        Value::Array(items) => items,
        Value::Object(_) => vec![value],
        _ => Vec::new(),
    }
}

fn value_items(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(items) => items.iter().collect(),
        Value::Object(_) => vec![value],
        _ => Vec::new(),
    }
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    match value.get(field)? {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
    .filter(|value| !value.trim().is_empty())
}

fn u64_field(value: &Value, field: &str) -> Option<u64> {
    match value.get(field)? {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn clean_scope_id(address: &str) -> String {
    address
        .split_once('%')
        .map(|(address, _)| address)
        .unwrap_or(address)
        .to_string()
}

fn infer_windows_interface_type(name: &str) -> InterfaceType {
    let lower = name.to_ascii_lowercase();
    if lower.contains("loopback") {
        InterfaceType::Loopback
    } else if lower.contains("tail")
        || lower.contains("vpn")
        || lower.contains("wireguard")
        || lower.contains("tun")
    {
        InterfaceType::Vpn
    } else if lower.contains("bridge") || lower.contains("hyper-v") || lower.contains("vethernet") {
        InterfaceType::Bridge
    } else {
        InterfaceType::WifiOrEthernet
    }
}

fn classify_windows_interface(
    name: &str,
    ipv4: &[InterfaceAddress],
    ipv6: &[InterfaceAddress],
) -> NetworkKind {
    let lower = name.to_ascii_lowercase();
    if lower.contains("loopback") || ipv4.iter().any(|addr| addr.value.starts_with("127.")) {
        NetworkKind::Loopback
    } else if lower.contains("tail")
        || lower.contains("vpn")
        || lower.contains("wireguard")
        || lower.contains("tun")
    {
        NetworkKind::Vpn
    } else if lower.contains("docker") || lower.contains("vethernet") || lower.contains("hyper-v") {
        NetworkKind::Container
    } else if ipv4.iter().any(|addr| is_private_ipv4(&addr.value)) {
        NetworkKind::Lan
    } else if ipv6.iter().any(|addr| addr.value.starts_with("fe80:")) {
        NetworkKind::LinkLocal
    } else {
        NetworkKind::Unknown
    }
}

fn is_private_ipv4(value: &str) -> bool {
    let octets: Vec<u8> = value
        .split('.')
        .filter_map(|part| part.parse::<u8>().ok())
        .collect();
    matches!(
        octets.as_slice(),
        [10, _, _, _] | [172, 16..=31, _, _] | [192, 168, _, _] | [169, 254, _, _]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_powershell_interface_json() {
        let input = r#"
[
  {
    "InterfaceAlias": "Wi-Fi",
    "NetAdapter": { "Status": "Up", "MacAddress": "A0-02-A5-78-76-7F", "MtuSize": 1500 },
    "IPv4Address": { "IPAddress": "192.168.200.154", "PrefixLength": 24 },
    "IPv6Address": { "IPAddress": "fe80::1%16", "PrefixLength": 64 },
    "IPv4DefaultGateway": { "NextHop": "192.168.200.254" }
  }
]
"#;

        let interfaces = parse_powershell_interfaces(input);

        assert_eq!(interfaces.len(), 1);
        assert_eq!(interfaces[0].name, "Wi-Fi");
        assert_eq!(interfaces[0].status, InterfaceStatus::Up);
        assert_eq!(
            interfaces[0].mac_address.as_deref(),
            Some("a0:02:a5:78:76:7f")
        );
        assert_eq!(interfaces[0].ipv4[0].value, "192.168.200.154");
        assert_eq!(interfaces[0].ipv4[0].prefix_len, Some(24));
        assert_eq!(
            interfaces[0].ipv4[0].gateway.as_deref(),
            Some("192.168.200.254")
        );
        assert_eq!(interfaces[0].ipv6[0].value, "fe80::1");
    }

    #[test]
    fn parses_powershell_route_json() {
        let input = r#"
[
  { "DestinationPrefix": "0.0.0.0/0", "NextHop": "192.168.200.254", "InterfaceAlias": "Wi-Fi", "RouteMetric": 35, "Protocol": "Dhcp", "AddressFamily": "IPv4" },
  { "DestinationPrefix": "fe80::/64", "NextHop": "::", "InterfaceIndex": 16, "RouteMetric": 291, "AddressFamily": "IPv6" }
]
"#;

        let routes = parse_powershell_routes(input);

        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].destination, "default");
        assert_eq!(routes[0].gateway, "192.168.200.254");
        assert_eq!(routes[0].interface, "Wi-Fi");
        assert_eq!(routes[0].family, RouteFamily::Ipv4);
        assert_eq!(routes[1].gateway, "link");
        assert_eq!(routes[1].family, RouteFamily::Ipv6);
    }

    #[test]
    fn parses_powershell_port_and_connection_json() {
        let ports = parse_powershell_listening_ports(
            r#"{ "LocalAddress": "127.0.0.1", "LocalPort": 5050, "OwningProcess": 2460 }"#,
        );
        let connections = parse_powershell_connections(
            r#"{ "LocalAddress": "127.0.0.1", "LocalPort": 24801, "RemoteAddress": "127.0.0.1", "RemotePort": 52108, "State": "Established" }"#,
        );

        assert_eq!(ports[0].local_port, "5050");
        assert_eq!(ports[0].pid, "2460");
        assert_eq!(connections[0].state.as_deref(), Some("Established"));
    }

    #[test]
    fn merges_powershell_interface_statistics() {
        let interfaces = vec![NetworkInterface {
            interface_type: InterfaceType::WifiOrEthernet,
            network_kind: NetworkKind::Lan,
            status: InterfaceStatus::Up,
            mtu: Some(1500),
            name: "Wi-Fi".to_string(),
            ipv4: Vec::new(),
            ipv6: Vec::new(),
            mac_address: None,
            stats: None,
        }];
        let stats_json = r#"
[
  {
    "Name": "Wi-Fi",
    "ReceivedBytes": 9178588284,
    "SentBytes": 4905232533,
    "ReceivedUnicastPackets": 11885040,
    "SentUnicastPackets": 9544740
  }
]
"#;

        let merged = merge_powershell_interface_stats(stats_json, interfaces);

        assert_eq!(
            merged[0].stats.as_ref(),
            Some(&InterfaceStats {
                rx_bytes: 9178588284,
                tx_bytes: 4905232533,
                rx_packets: 11885040,
                tx_packets: 9544740,
            })
        );
    }

    #[test]
    fn merges_windows_interface_statistics_by_alias_or_description() {
        let interfaces = vec![
            NetworkInterface {
                interface_type: InterfaceType::WifiOrEthernet,
                network_kind: NetworkKind::Lan,
                status: InterfaceStatus::Up,
                mtu: Some(1500),
                name: "Wi-Fi".to_string(),
                ipv4: Vec::new(),
                ipv6: Vec::new(),
                mac_address: None,
                stats: None,
            },
            NetworkInterface {
                interface_type: InterfaceType::WifiOrEthernet,
                network_kind: NetworkKind::Lan,
                status: InterfaceStatus::Up,
                mtu: Some(1500),
                name: "USB Ethernet".to_string(),
                ipv4: Vec::new(),
                ipv6: Vec::new(),
                mac_address: None,
                stats: None,
            },
        ];
        let wifi_stats = InterfaceStats {
            rx_bytes: 1000,
            tx_bytes: 2000,
            rx_packets: 10,
            tx_packets: 20,
        };
        let usb_stats = InterfaceStats {
            rx_bytes: 3000,
            tx_bytes: 4000,
            rx_packets: 30,
            tx_packets: 40,
        };
        let stats = vec![
            WindowsInterfaceStats {
                alias: "Wi-Fi".to_string(),
                description: "Intel(R) Wi-Fi 6".to_string(),
                stats: wifi_stats.clone(),
            },
            WindowsInterfaceStats {
                alias: "Ethernet 2".to_string(),
                description: "USB Ethernet".to_string(),
                stats: usb_stats.clone(),
            },
        ];

        let merged = merge_windows_interface_stats(interfaces, &stats);

        assert_eq!(merged[0].stats.as_ref(), Some(&wifi_stats));
        assert_eq!(merged[1].stats.as_ref(), Some(&usb_stats));
    }
}
