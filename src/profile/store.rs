use std::path::{Path, PathBuf};

use crate::profile::ProfileConfig;

pub fn config_path_for_base(base: &Path) -> PathBuf {
    base.join("lazyifconfig").join("config.toml")
}

pub fn default_profile_path_for_base(base: &Path) -> PathBuf {
    profile_path_for_base(base, "default")
}

pub fn profile_path_for_base(base: &Path, profile_name: &str) -> PathBuf {
    base.join("lazyifconfig")
        .join("profiles")
        .join(format!("{profile_name}.toml"))
}

pub fn load_profile_from_path(path: &Path) -> Result<ProfileConfig, String> {
    let contents = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    ProfileConfig::from_toml_str(&contents)
}
