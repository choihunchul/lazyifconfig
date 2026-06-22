use crate::model::ActiveConnection;

pub fn parse_connections(input: &str) -> Vec<ActiveConnection> {
    let mut connections = Vec::new();

    for line in input.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let proto = parts[0].to_lowercase();
        if !proto.starts_with("tcp") && !proto.starts_with("udp") {
            continue;
        }

        if parts.len() < 4 {
            continue;
        }

        let windows_netstat_row = parts
            .get(1)
            .zip(parts.get(2))
            .is_some_and(|(local, foreign)| local.contains(':') && foreign.contains(':'));
        let (local_addr, foreign_addr, state) = if windows_netstat_row {
            (
                parts[1],
                parts[2],
                proto.starts_with("tcp").then(|| parts[3].to_string()),
            )
        } else if parts.len() >= 5 {
            (
                parts[3],
                parts[4],
                (proto.starts_with("tcp") && parts.len() >= 6).then(|| parts[5].to_string()),
            )
        } else {
            continue;
        };

        let (local_ip, local_port) = split_ip_port(local_addr);
        let (foreign_ip, foreign_port) = split_ip_port(foreign_addr);

        connections.push(ActiveConnection {
            proto,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_korean_windows_netstat_connections() {
        let input = "\
활성 연결

  프로토콜  로컬 주소              외부 주소              상태            PID
  TCP    127.0.0.1:24801        127.0.0.1:52108        ESTABLISHED     4684
  TCP    192.168.200.154:55500  20.184.175.16:443      ESTABLISHED
  TCP    192.168.200.154:55500  20.184.175.16:443      ESTABLISHED     13624
";

        let connections = parse_connections(input);

        assert_eq!(connections.len(), 3);
        assert_eq!(connections[0].proto, "tcp");
        assert_eq!(connections[0].local_ip, "127.0.0.1");
        assert_eq!(connections[0].local_port, "24801");
        assert_eq!(connections[0].foreign_ip, "127.0.0.1");
        assert_eq!(connections[0].foreign_port, "52108");
        assert_eq!(connections[0].state.as_deref(), Some("ESTABLISHED"));
    }
}
