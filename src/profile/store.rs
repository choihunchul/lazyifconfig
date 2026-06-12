use std::path::{Path, PathBuf};

use crate::profile::ProfileConfig;

pub fn config_path_for_base(base: &Path) -> PathBuf {
    base.join("lazyifconfig").join("config.toml")
}

pub fn config_base_dir() -> PathBuf {
    let env = [
        ("APPDATA", std::env::var("APPDATA").ok()),
        ("USERPROFILE", std::env::var("USERPROFILE").ok()),
        ("XDG_CONFIG_HOME", std::env::var("XDG_CONFIG_HOME").ok()),
        ("HOME", std::env::var("HOME").ok()),
    ];
    let env_refs = env
        .iter()
        .filter_map(|(key, value)| value.as_deref().map(|value| (*key, value)))
        .collect::<Vec<_>>();
    config_base_dir_for_env(std::env::consts::OS, &env_refs)
}

pub fn config_base_dir_for_env(os: &str, env: &[(&str, &str)]) -> PathBuf {
    if os == "windows" {
        if let Some(path) = env_value(env, "APPDATA") {
            return PathBuf::from(path);
        }
        if let Some(profile) = env_value(env, "USERPROFILE") {
            return PathBuf::from(profile).join("AppData").join("Roaming");
        }
        return PathBuf::from(".");
    }

    if let Some(path) = env_value(env, "XDG_CONFIG_HOME") {
        return PathBuf::from(path);
    }

    if let Some(home) = env_value(env, "HOME") {
        return PathBuf::from(home).join(".config");
    }

    PathBuf::from(".")
}

fn env_value<'a>(env: &'a [(&str, &str)], key: &str) -> Option<&'a str> {
    env.iter()
        .find(|(candidate, value)| *candidate == key && !value.trim().is_empty())
        .map(|(_, value)| *value)
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

pub fn list_profile_names_for_base(base: &Path) -> Result<Vec<String>, String> {
    let dir = base.join("lazyifconfig").join("profiles");
    let mut names = vec!["default".to_string()];
    if !dir.exists() {
        return Ok(names);
    }

    for entry in std::fs::read_dir(&dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
            let name = stem.to_string();
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }

    names.sort();
    if let Some(default_index) = names.iter().position(|name| name == "default") {
        names.remove(default_index);
    }
    names.insert(0, "default".to_string());
    Ok(names)
}

pub fn save_profile_to_base(base: &Path, profile: &ProfileConfig) -> Result<PathBuf, String> {
    validate_profile_name(&profile.profile.name)?;
    let path = profile_path_for_base(base, &profile.profile.name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let toml = profile.to_toml_string()?;
    std::fs::write(&path, toml).map_err(|error| error.to_string())?;
    Ok(path)
}

fn validate_profile_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Profile name is required.".to_string());
    }
    if name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        Ok(())
    } else {
        Err("Profile name may only use letters, numbers, '-' and '_'.".to_string())
    }
}
