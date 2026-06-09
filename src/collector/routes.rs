use crate::model::RouteEntry;

pub fn parse_routes(netstat_output: &str) -> Vec<RouteEntry> {
    let mut routes = Vec::new();
    let mut parsing_ipv4 = false;

    for line in netstat_output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Routing tables") {
            continue;
        }
        if trimmed.starts_with("Internet:") {
            parsing_ipv4 = true;
            continue;
        } else if trimmed.starts_with("Internet6:") {
            parsing_ipv4 = false;
            continue;
        }

        if parsing_ipv4 {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let destination = parts[0];
                let gateway = parts[1];
                let _flags = parts[2];
                let interface = parts[3];

                // Skip headers
                if destination == "Destination" {
                    continue;
                }

                routes.push(RouteEntry {
                    destination: destination.to_string(),
                    gateway: gateway.to_string(),
                    interface: interface.to_string(),
                });
            }
        }
    }
    routes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_routes() {
        let sample = "\
Routing tables

Internet:
Destination        Gateway            Flags               Netif Expire
default            192.168.0.1        UGScg                 en0
127.0.0.1          127.0.0.1          UH                    lo0
192.168.0.0/24     link#18            UCS                   en0

Internet6:
Destination        Gateway            Flags         Netif Expire
::1                ::1                UHL            lo0
";
        let routes = parse_routes(sample);
        assert_eq!(routes.len(), 3);
        assert_eq!(routes[0].destination, "default");
        assert_eq!(routes[0].gateway, "192.168.0.1");
        assert_eq!(routes[0].interface, "en0");

        assert_eq!(routes[1].destination, "127.0.0.1");
        assert_eq!(routes[1].gateway, "127.0.0.1");
        assert_eq!(routes[1].interface, "lo0");

        assert_eq!(routes[2].destination, "192.168.0.0/24");
        assert_eq!(routes[2].gateway, "link#18");
        assert_eq!(routes[2].interface, "en0");
    }
}
