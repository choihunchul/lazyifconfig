#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ProfileConfig {
    pub profile: ProfileDocument,
    #[serde(default)]
    pub networks: Vec<ProfileNetwork>,
    #[serde(default)]
    pub hosts: Vec<ProfileHost>,
    #[serde(default)]
    pub targets: Vec<ProfileTarget>,
}

impl ProfileConfig {
    pub fn from_toml_str(input: &str) -> Result<Self, String> {
        toml::from_str(input).map_err(|error| error.to_string())
    }

    pub fn to_toml_string(&self) -> Result<String, String> {
        toml::to_string_pretty(self).map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ProfileDocument {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub auto_detect: ProfileAutoDetect,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileAutoDetect {
    Off,
    #[default]
    Prompt,
    Auto,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ProfileNetwork {
    pub cidr: String,
    pub name: String,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ProfileHost {
    pub ip: String,
    pub name: String,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ProfileTarget {
    pub name: String,
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub kind: Option<String>,
}
