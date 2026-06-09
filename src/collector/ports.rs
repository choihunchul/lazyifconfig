use crate::model::ListeningPort;

pub fn parse_listening_ports(input: &str) -> Vec<ListeningPort> {
    let mut ports = Vec::new();
    let lines: Vec<&str> = input.lines().collect();
    if lines.is_empty() {
        return ports;
    }

    // Skip header line
    for line in lines.iter().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }

        let command = parts[0].to_string();
        let pid = parts[1].to_string();
        let user = parts[2].to_string();
        let proto = parts[7].to_lowercase();

        // The Node Name (parts[8]) usually looks like `*:56642` or `127.0.0.1:80`
        // It might end with ` (LISTEN)` or `(LISTEN)`
        let name_part = parts[8];
        let name_clean = name_part
            .trim_end_matches(" (LISTEN)")
            .trim_end_matches("(LISTEN)")
            .trim();

        let (local_ip, local_port) = split_node_name(name_clean);

        ports.push(ListeningPort {
            proto,
            local_ip,
            local_port,
            pid,
            command,
            user,
        });
    }

    ports
}

fn split_node_name(node: &str) -> (String, String) {
    if let Some(pos) = node.rfind(':') {
        let ip = node[..pos].to_string();
        let port = node[pos + 1..].to_string();
        (ip, port)
    } else {
        (node.to_string(), "*".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lsof_listening_tcp_rows() {
        let input = "\
COMMAND   PID USER   FD   TYPE             DEVICE SIZE/OFF NODE NAME
node    12345 user   21u  IPv6 0x123456789abcdef0      0t0  TCP *:3000 (LISTEN)
Python  23456 user    5u  IPv4 0xabcdef0123456789      0t0  TCP 127.0.0.1:8000 (LISTEN)
";

        let ports = parse_listening_ports(input);

        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].proto, "tcp");
        assert_eq!(ports[0].local_ip, "*");
        assert_eq!(ports[0].local_port, "3000");
        assert_eq!(ports[0].pid, "12345");
        assert_eq!(ports[0].command, "node");
        assert_eq!(ports[0].user, "user");

        assert_eq!(ports[1].proto, "tcp");
        assert_eq!(ports[1].local_ip, "127.0.0.1");
        assert_eq!(ports[1].local_port, "8000");
    }
}
