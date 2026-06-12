pub mod join;
pub mod r#match;
pub mod model;
pub mod store;

pub use join::{label_ip, ProfileIpLabel};
pub use model::{
    ProfileAutoDetect, ProfileConfig, ProfileDocument, ProfileHost, ProfileNetwork, ProfileTarget,
};
pub use r#match::{suggest_profile, ProfileSuggestion, ProfileSuggestionInput};
pub use store::{
    config_base_dir, config_path_for_base, default_profile_path_for_base,
    list_profile_names_for_base, load_profile_from_path, profile_path_for_base,
    save_profile_to_base,
};
