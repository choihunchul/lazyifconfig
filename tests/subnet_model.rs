use lazyifconfig::model::Subnet;
use std::net::{Ipv4Addr, Ipv6Addr};

#[test]
fn test_subnet_sorting_order() {
    let ip4_1 = Subnet::Ipv4 { network: Ipv4Addr::new(10, 0, 0, 0), prefix_len: 8 };
    let ip4_2 = Subnet::Ipv4 { network: Ipv4Addr::new(192, 168, 0, 0), prefix_len: 24 };
    let ip6_1 = Subnet::Ipv6 { network: Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0), prefix_len: 64 };
    let unassigned = Subnet::Unassigned;

    let mut subnets = vec![unassigned.clone(), ip6_1.clone(), ip4_2.clone(), ip4_1.clone()];
    subnets.sort();

    // 정렬 기대 순서: IPv4 -> IPv6 -> Unassigned
    assert_eq!(subnets[0], ip4_1);
    assert_eq!(subnets[1], ip4_2);
    assert_eq!(subnets[2], ip6_1);
    assert_eq!(subnets[3], unassigned);
}
