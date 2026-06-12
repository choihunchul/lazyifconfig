use lazyifconfig::model::RouteEntry;
use lazyifconfig::profile::{label_ip, suggest_profile, ProfileConfig, ProfileSuggestionInput};

fn profile(name: &str, cidr: &str, gateway: &str) -> ProfileConfig {
    ProfileConfig::from_toml_str(&format!(
        r#"
[profile]
name = "{name}"

[[networks]]
cidr = "{cidr}"
name = "{name} LAN"
kind = "lan"

[[hosts]]
ip = "{gateway}"
name = "{name}-gateway"
role = "gateway"
"#
    ))
    .unwrap()
}

#[test]
fn label_ip_prefers_exact_host_then_network() {
    let office = profile("office", "10.20.0.0/16", "10.20.1.1");

    assert_eq!(
        label_ip("10.20.1.1", &office).unwrap().display,
        "office-gateway"
    );
    assert_eq!(
        label_ip("10.20.4.82", &office).unwrap().display,
        "office LAN"
    );
    assert!(label_ip("8.8.8.8", &office).is_none());
}

#[test]
fn suggestion_scores_fast_local_signals() {
    let office = profile("office", "10.20.0.0/16", "10.20.1.1");
    let home = profile("home", "192.168.0.0/24", "192.168.0.1");

    let input = ProfileSuggestionInput {
        interface_ips: vec!["10.20.4.82".to_string()],
        gateways: vec!["10.20.1.1".to_string()],
        routes: vec![RouteEntry::new("10.30.0.0/16", "10.20.1.1", "en0")],
    };

    let suggestion = suggest_profile(&[home, office], &input).expect("suggestion exists");

    assert_eq!(suggestion.profile_name, "office");
    assert!(suggestion.score >= 80);
    assert!(suggestion
        .reasons
        .iter()
        .any(|reason| reason.contains("network")));
    assert!(suggestion
        .reasons
        .iter()
        .any(|reason| reason.contains("gateway")));
}
