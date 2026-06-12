use crate::profile::r#match::ip_in_cidr;
use crate::profile::ProfileConfig;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileIpLabel {
    pub display: String,
    pub kind: Option<String>,
}

pub fn label_ip(ip: &str, profile: &ProfileConfig) -> Option<ProfileIpLabel> {
    if let Some(host) = profile.hosts.iter().find(|host| host.ip == ip) {
        return Some(ProfileIpLabel {
            display: host.name.clone(),
            kind: host.role.clone(),
        });
    }

    profile
        .networks
        .iter()
        .find(|network| ip_in_cidr(ip, &network.cidr))
        .map(|network| ProfileIpLabel {
            display: network.name.clone(),
            kind: network.kind.clone(),
        })
}
