use std::path::PathBuf;

use lazyifconfig::profile::{
    config_path_for_base, default_profile_path_for_base, profile_path_for_base,
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
