pub mod join;
pub mod r#match;
pub mod model;

pub use join::{label_ip, ProfileIpLabel};
pub use model::{
    ProfileAutoDetect, ProfileConfig, ProfileDocument, ProfileHost, ProfileNetwork, ProfileTarget,
};
pub use r#match::{suggest_profile, ProfileSuggestion, ProfileSuggestionInput};
