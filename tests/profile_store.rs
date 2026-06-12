use std::path::PathBuf;

use lazyifconfig::profile::{
    config_path_for_base, default_profile_path_for_base, list_profile_names_for_base,
    profile_path_for_base, save_profile_to_base, ProfileConfig,
};

#[test]
fn profile_paths_are_under_lazyifconfig_config_root() {
    let base = PathBuf::from("/tmp/user-config");

    assert_eq!(
        config_path_for_base(&base),
        PathBuf::from("/tmp/user-config/lazyifconfig/config.toml")
    );
    assert_eq!(
        default_profile_path_for_base(&base),
        PathBuf::from("/tmp/user-config/lazyifconfig/profiles/default.toml")
    );
    assert_eq!(
        profile_path_for_base(&base, "office"),
        PathBuf::from("/tmp/user-config/lazyifconfig/profiles/office.toml")
    );
}

#[test]
fn save_profile_then_list_profile_names() {
    let base = std::env::temp_dir().join(format!(
        "lazyifconfig-profile-store-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);

    let profile = ProfileConfig::from_toml_str(
        r#"
[profile]
name = "office"
"#,
    )
    .unwrap();

    save_profile_to_base(&base, &profile).expect("profile saves");
    let names = list_profile_names_for_base(&base).expect("profiles list");

    assert_eq!(names, vec!["default".to_string(), "office".to_string()]);

    let _ = std::fs::remove_dir_all(&base);
}
