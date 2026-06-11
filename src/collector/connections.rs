use crate::model::ActiveConnection;

pub fn parse_connections(input: &str) -> Vec<ActiveConnection> {
    let mut connections = Vec::new();

    for line in input.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let proto = parts[0];
        if !proto.starts_with("tcp") && !proto.starts_with("udp") {
            continue;
        }

        if parts.len() < 5 {
            continue;
        }

        let local_addr = parts[3];
        let foreign_addr = parts[4];
        let state = if proto.starts_with("tcp") && parts.len() >= 6 {
            Some(parts[5].to_string())
        } else {
            None
        };

        let (local_ip, local_port) = split_ip_port(local_addr);
        let (foreign_ip, foreign_port) = split_ip_port(foreign_addr);

        connections.push(ActiveConnection {
            proto: proto.to_string(),
            local_ip,
            local_port,
            foreign_ip,
            foreign_port,
            state,
        });
    }

    connections
}

fn split_ip_port(addr: &str) -> (String, String) {
    if let Some(pos) = addr.rfind(':') {
        let ip = clean_socket_ip(&addr[..pos]);
        let port = addr[pos + 1..].to_string();
        if !port.is_empty() {
            return (ip, port);
        }
    }

    if let Some(pos) = addr.rfind('.') {
        let ip = addr[..pos].to_string();
        let port = addr[pos + 1..].to_string();
        (ip, port)
    } else {
        (addr.to_string(), "*".to_string())
    }
}

fn clean_socket_ip(ip: &str) -> String {
    ip.trim_start_matches('[').trim_end_matches(']').to_string()
}
