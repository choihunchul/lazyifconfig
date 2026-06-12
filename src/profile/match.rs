use std::net::Ipv4Addr;

use crate::model::RouteEntry;
use crate::profile::ProfileConfig;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileSuggestionInput {
    pub interface_ips: Vec<String>,
    pub gateways: Vec<String>,
    pub routes: Vec<RouteEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileSuggestion {
    pub profile_name: String,
    pub score: u32,
    pub reasons: Vec<String>,
}

pub fn suggest_profile(
    profiles: &[ProfileConfig],
    input: &ProfileSuggestionInput,
) -> Option<ProfileSuggestion> {
    profiles
        .iter()
        .map(|profile| score_profile(profile, input))
        .filter(|suggestion| suggestion.score > 0)
        .max_by_key(|suggestion| suggestion.score)
}

fn score_profile(profile: &ProfileConfig, input: &ProfileSuggestionInput) -> ProfileSuggestion {
    let mut score = 0;
    let mut reasons = Vec::new();

    for ip in &input.interface_ips {
        if profile
            .networks
            .iter()
            .any(|network| ip_in_cidr(ip, &network.cidr))
        {
            score += 50;
            reasons.push(format!("interface network matched {ip}"));
            break;
        }
    }

    for gateway in &input.gateways {
        if profile.hosts.iter().any(|host| host.ip == *gateway) {
            score += 30;
            reasons.push(format!("gateway matched {gateway}"));
            break;
        }
    }

    for route in &input.routes {
        if profile
            .networks
            .iter()
            .any(|network| network.cidr == route.destination)
        {
            score += 20;
            reasons.push(format!("route matched {}", route.destination));
            break;
        }
    }

    ProfileSuggestion {
        profile_name: profile.profile.name.clone(),
        score,
        reasons,
    }
}

pub fn ip_in_cidr(ip: &str, cidr: &str) -> bool {
    let Ok(ip) = ip.parse::<Ipv4Addr>() else {
        return false;
    };
    let Some((network, prefix)) = cidr.split_once('/') else {
        return false;
    };
    let Ok(network) = network.parse::<Ipv4Addr>() else {
        return false;
    };
    let Ok(prefix) = prefix.parse::<u8>() else {
        return false;
    };
    if prefix > 32 {
        return false;
    }

    let ip = u32::from(ip);
    let network = u32::from(network);
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };

    (ip & mask) == (network & mask)
}
