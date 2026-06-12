use std::path::{Path, PathBuf};

use crate::profile::ProfileConfig;

pub fn config_path_for_base(base: &Path) -> PathBuf {
    base.join("lazyifconfig").join("config.toml")
}

pub fn config_base_dir() -> PathBuf {
    if let Ok(path) = std::env::var("XDG_CONFIG_HOME") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home).join(".config");
        }
    }

    PathBuf::from(".")
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
