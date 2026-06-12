use lazyifconfig::profile::{
    ProfileAutoDetect, ProfileConfig, ProfileHost, ProfileNetwork, ProfileTarget,
};

#[test]
fn parses_profile_toml_with_networks_hosts_and_targets() {
    let input = r#"
[profile]
name = "office"
description = "Office network"
auto_detect = "prompt"

[[networks]]
cidr = "10.20.0.0/16"
name = "Office LAN"
kind = "lan"

[[hosts]]
ip = "10.20.1.1"
name = "office-gateway"
role = "gateway"

[[targets]]
name = "Staging API"
host = "staging-api.internal.company.com"
port = 443
kind = "service"
"#;

    let parsed = ProfileConfig::from_toml_str(input).expect("profile parses");

    assert_eq!(parsed.profile.name, "office");
    assert_eq!(parsed.profile.description.as_deref(), Some("Office network"));
    assert_eq!(parsed.profile.auto_detect, ProfileAutoDetect::Prompt);
    assert_eq!(
        parsed.networks,
        vec![ProfileNetwork {
            cidr: "10.20.0.0/16".to_string(),
            name: "Office LAN".to_string(),
            kind: Some("lan".to_string()),
        }]
    );
    assert_eq!(
        parsed.hosts,
        vec![ProfileHost {
            ip: "10.20.1.1".to_string(),
            name: "office-gateway".to_string(),
            role: Some("gateway".to_string()),
        }]
    );
    assert_eq!(
        parsed.targets,
        vec![ProfileTarget {
            name: "Staging API".to_string(),
            host: "staging-api.internal.company.com".to_string(),
            port: Some(443),
            kind: Some("service".to_string()),
        }]
    );
}

#[test]
fn missing_lists_default_to_empty() {
    let parsed = ProfileConfig::from_toml_str(
        r#"
[profile]
name = "default"
"#,
    )
    .expect("profile parses");

    assert!(parsed.networks.is_empty());
    assert!(parsed.hosts.is_empty());
    assert!(parsed.targets.is_empty());
}
