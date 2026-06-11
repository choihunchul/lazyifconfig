use std::collections::HashMap;

use crate::model::{InterfaceStats, NetworkInterface};

pub fn parse_stats(input: &str) -> HashMap<String, InterfaceStats> {
    let mut current_name: Option<String> = None;
    let mut pending_ip_stat: Option<StatDirection> = None;
    let mut stats_by_name: HashMap<String, InterfaceStats> = HashMap::new();

    for line in input.lines() {
        if let Some(name) = parse_interface_header_name(line) {
            current_name = Some(name);
            pending_ip_stat = None;
            continue;
        }

        let Some(name) = current_name.as_ref() else {
            continue;
        };

        let trimmed = line.trim();
        if trimmed.starts_with("RX:") {
            pending_ip_stat = Some(StatDirection::Rx);
            continue;
        } else if trimmed.starts_with("TX:") {
            pending_ip_stat = Some(StatDirection::Tx);
            continue;
        }

        if let Some(direction) = pending_ip_stat.take() {
            let Some((packets, bytes)) = parse_ip_stat_values(trimmed) else {
                continue;
            };
            let stats = stats_by_name.entry(name.clone()).or_default();
            match direction {
                StatDirection::Rx => {
                    stats.rx_packets = packets;
                    stats.rx_bytes = bytes;
                }
                StatDirection::Tx => {
                    stats.tx_packets = packets;
                    stats.tx_bytes = bytes;
                }
            }
            continue;
        }

        let Some((direction, packets, bytes)) = parse_stat_line(trimmed) else {
            continue;
        };

        let stats = stats_by_name.entry(name.clone()).or_default();
        match direction {
            StatDirection::Rx => {
                stats.rx_packets = packets;
                stats.rx_bytes = bytes;
            }
            StatDirection::Tx => {
                stats.tx_packets = packets;
                stats.tx_bytes = bytes;
            }
        }
    }

    stats_by_name
}

pub fn merge_stats(input: &str, mut interfaces: Vec<NetworkInterface>) -> Vec<NetworkInterface> {
    let stats_by_name = if input.contains("Ibytes") && input.contains("Obytes") {
        parse_netstat_ib(input)
    } else {
        parse_stats(input)
    };

    for interface in &mut interfaces {
        if let Some(stats) = stats_by_name.get(&interface.name) {
            interface.stats = Some(stats.clone());
        }
    }

    interfaces
}

fn parse_netstat_ib(input: &str) -> HashMap<String, InterfaceStats> {
    let mut stats_by_name: HashMap<String, InterfaceStats> = HashMap::new();

    for line in input.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 10 {
            continue;
        }

        // We only care about Link-level stats row, which has `<Link#` in the Network column (parts[2])
        if !parts[2].starts_with("<Link#") {
            continue;
        }

        let name = parts[0].trim_end_matches('*').to_string();
        let len = parts.len();

        let rx_packets = match parts[len - 7].parse::<u64>() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let rx_bytes = match parts[len - 5].parse::<u64>() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let tx_packets = match parts[len - 4].parse::<u64>() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let tx_bytes = match parts[len - 2].parse::<u64>() {
            Ok(v) => v,
            Err(_) => continue,
        };

        stats_by_name.insert(
            name,
            InterfaceStats {
                rx_packets,
                rx_bytes,
                tx_packets,
                tx_bytes,
            },
        );
    }

    stats_by_name
}

fn parse_interface_header_name(line: &str) -> Option<String> {
    if line.starts_with(' ') || line.starts_with('\t') {
        return None;
    }

    let (first, rest) = line.split_once(':')?;
    if first.chars().all(|c| c.is_ascii_digit()) {
        let (name, _) = rest.trim_start().split_once(':')?;
        return Some(clean_interface_name(name));
    }

    Some(clean_interface_name(first))
}

fn clean_interface_name(name: &str) -> String {
    name.trim()
        .split('@')
        .next()
        .unwrap_or_default()
        .to_string()
}

fn parse_stat_line(line: &str) -> Option<(StatDirection, u64, u64)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 5 || parts[1] != "packets" || parts[3] != "bytes" {
        return None;
    }

    let packets = parts[2].parse().ok()?;
    let bytes = parts[4].parse().ok()?;

    match parts[0] {
        "RX" => Some((StatDirection::Rx, packets, bytes)),
        "TX" => Some((StatDirection::Tx, packets, bytes)),
        _ => None,
    }
}

fn parse_ip_stat_values(line: &str) -> Option<(u64, u64)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let bytes = parts[0].parse().ok()?;
    let packets = parts[1].parse().ok()?;
    Some((packets, bytes))
}

#[derive(Clone, Copy)]
enum StatDirection {
    Rx,
    Tx,
}
