use lazyifconfig::collector::routes::{
    parse_linux_route_path,
    parse_macos_route_path,
    parse_routes,
};
use lazyifconfig::model::RouteFamily;

#[test]
fn parses_macos_ipv4_and_ipv6_routes_with_metadata() {
    let sample = "\
Routing tables

Internet:
Destination        Gateway            Flags               Netif Expire
default            192.168.0.1        UGScg                 en0
10.8.0.0/24        link#20            UCS                 utun4

Internet6:
Destination                             Gateway                         Flags               Netif Expire
default                                 fe80::1%en0                     UGcI                  en0
::1                                     ::1                             UHL                   lo0
";

    let routes = parse_routes(sample);

    assert_eq!(routes.len(), 4);
    assert_eq!(routes[0].destination, "default");
    assert_eq!(routes[0].gateway, "192.168.0.1");
    assert_eq!(routes[0].interface, "en0");
    assert_eq!(routes[0].flags.as_deref(), Some("UGScg"));
    assert_eq!(routes[0].family, RouteFamily::Ipv4);

    assert_eq!(routes[2].destination, "default");
    assert_eq!(routes[2].gateway, "fe80::1%en0");
    assert_eq!(routes[2].interface, "en0");
    assert_eq!(routes[2].family, RouteFamily::Ipv6);
}

#[test]
fn parses_linux_ipv4_routes_with_metric_and_protocol() {
    let sample = "\
default via 172.17.0.1 dev eth0 proto static metric 100
172.17.0.0/16 dev eth0 proto kernel scope link src 172.17.0.2
10.8.0.0/24 via 10.8.0.1 dev tun0 metric 50
";

    let routes = parse_routes(sample);

    assert_eq!(routes.len(), 3);
    assert_eq!(routes[0].destination, "default");
    assert_eq!(routes[0].gateway, "172.17.0.1");
    assert_eq!(routes[0].interface, "eth0");
    assert_eq!(routes[0].protocol.as_deref(), Some("static"));
    assert_eq!(routes[0].metric, Some(100));
    assert_eq!(routes[0].family, RouteFamily::Ipv4);

    assert_eq!(routes[2].destination, "10.8.0.0/24");
    assert_eq!(routes[2].gateway, "10.8.0.1");
    assert_eq!(routes[2].interface, "tun0");
    assert_eq!(routes[2].metric, Some(50));
}

#[test]
fn parses_linux_ipv6_routes() {
    let sample = "\
default via fe80::1 dev eth0 proto ra metric 1024
fe80::/64 dev eth0 proto kernel metric 256
";

    let routes = parse_routes(sample);

    assert_eq!(routes.len(), 2);
    assert_eq!(routes[0].family, RouteFamily::Ipv6);
    assert_eq!(routes[0].gateway, "fe80::1");
    assert_eq!(routes[0].metric, Some(1024));
}

#[test]
fn parses_linux_route_get_output() {
    let output = "8.8.8.8 via 172.17.0.1 dev eth0 src 172.17.0.2 uid 501\n    cache";

    let result = parse_linux_route_path("8.8.8.8", output).unwrap();

    assert_eq!(result.destination, "8.8.8.8");
    assert_eq!(result.resolved_destination.as_deref(), Some("8.8.8.8"));
    assert_eq!(result.gateway.as_deref(), Some("172.17.0.1"));
    assert_eq!(result.interface.as_deref(), Some("eth0"));
    assert_eq!(result.source_ip.as_deref(), Some("172.17.0.2"));
    assert_eq!(result.raw_output, output);
}

#[test]
fn parses_macos_route_get_output() {
    let output = "\
   route to: 8.8.8.8
destination: default
       mask: default
    gateway: 192.168.0.1
  interface: en0
      flags: <UP,GATEWAY,DONE,STATIC,PRCLONING>
 recvpipe  sendpipe  ssthresh  rtt,msec    rttvar  hopcount      mtu     expire
       0         0         0         0         0         0      1500         0
";

    let result = parse_macos_route_path("8.8.8.8", output).unwrap();

    assert_eq!(result.destination, "8.8.8.8");
    assert_eq!(result.resolved_destination.as_deref(), Some("8.8.8.8"));
    assert_eq!(result.gateway.as_deref(), Some("192.168.0.1"));
    assert_eq!(result.interface.as_deref(), Some("en0"));
    assert_eq!(result.raw_output, output);
}
