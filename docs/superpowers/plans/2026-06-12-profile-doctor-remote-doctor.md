# Profile, Doctor, Remote Doctor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build profile-aware network labeling first, then local Doctor diagnosis, then Windows-friendly Remote Doctor report import without requiring profile copies or lazyifconfig installs on remote machines.

**Architecture:** Add a small `profile` module that owns TOML loading, matching, labels, active profile state, and profile detection. Existing collectors keep producing raw network state; profile logic joins labels onto current state at render/diagnosis time so profile switches do not require re-running OS commands. Doctor later consumes the same `NetworkSnapshot` plus active profile; Remote Doctor changes only the snapshot source by importing a collected report first, with SSH/WinRM left as later transport options.

**Tech Stack:** Rust 2021, Ratatui, Crossterm, Tokio, Serde, TOML via `toml`, existing collector/parser modules, existing app state and tests.

---

## Scope Split

This roadmap has six working slices:

1. Profile Core
2. Profile Switcher
3. Profile Detection
4. Profile Editor MVP
5. Local Doctor
6. Remote Doctor Report Import

Implement in order. Each slice must compile, pass focused tests, and keep existing behavior if no profile exists.

## File Structure

- Create `src/profile/mod.rs`: public profile API and exports.
- Create `src/profile/model.rs`: profile TOML structs, labels, validation warnings.
- Create `src/profile/store.rs`: config/profile path resolution, load/save active profile, load profile files.
- Create `src/profile/match.rs`: CIDR parsing, IP-to-network/host matching, fast profile suggestion scoring.
- Create `src/profile/join.rs`: helper methods used by UI to format profile-aware labels.
- Create `tests/profile_model.rs`: TOML parsing and validation tests.
- Create `tests/profile_matching.rs`: CIDR, host, gateway, route, and suggestion tests.
- Modify `Cargo.toml`: add `toml = "0.8"` dependency.
- Modify `src/lib.rs`: export `profile`.
- Modify `src/app.rs`: add active profile state, profile switcher state, profile-aware helper methods.
- Modify `src/main.rs`: parse `--profile`, initialize profile state, handle `P` switcher key later.
- Modify `src/ui.rs`: show current profile in header/status.
- Modify `src/ui/tables.rs`: show profile labels in connection/route tables.
- Modify `src/ui/details.rs`: show profile labels in selected detail panels.
- Later create `src/doctor/mod.rs`, `src/doctor/checks.rs`, `src/doctor/model.rs`.
- Later create `src/remote/mod.rs`, `src/remote/report.rs`, `src/remote/windows.rs`.

## Profile TOML MVP

```toml
[profile]
name = "office"
description = "Office network"
auto_detect = "prompt"

[[networks]]
cidr = "10.20.0.0/16"
name = "Office LAN"
kind = "lan"

[[hosts]]
ip = "10.20.1.1"
name = "office-gateway"
role = "gateway"

[[targets]]
name = "Staging API"
host = "staging-api.internal.company.com"
port = 443
kind = "service"
```

## Task 1: Profile Model and TOML Parsing

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lib.rs`
- Create: `src/profile/mod.rs`
- Create: `src/profile/model.rs`
- Test: `tests/profile_model.rs`

- [x] **Step 1: Add failing model tests**

Create `tests/profile_model.rs`:

```rust
use lazyifconfig::profile::{
    ProfileAutoDetect, ProfileConfig, ProfileHost, ProfileNetwork, ProfileTarget,
};

#[test]
fn parses_profile_toml_with_networks_hosts_and_targets() {
    let input = r#"
[profile]
name = "office"
description = "Office network"
auto_detect = "prompt"

[[networks]]
cidr = "10.20.0.0/16"
name = "Office LAN"
kind = "lan"

[[hosts]]
ip = "10.20.1.1"
name = "office-gateway"
role = "gateway"

[[targets]]
name = "Staging API"
host = "staging-api.internal.company.com"
port = 443
kind = "service"
"#;

    let parsed = ProfileConfig::from_toml_str(input).expect("profile parses");

    assert_eq!(parsed.profile.name, "office");
    assert_eq!(parsed.profile.description.as_deref(), Some("Office network"));
    assert_eq!(parsed.profile.auto_detect, ProfileAutoDetect::Prompt);
    assert_eq!(
        parsed.networks,
        vec![ProfileNetwork {
            cidr: "10.20.0.0/16".to_string(),
            name: "Office LAN".to_string(),
            kind: Some("lan".to_string()),
        }]
    );
    assert_eq!(
        parsed.hosts,
        vec![ProfileHost {
            ip: "10.20.1.1".to_string(),
            name: "office-gateway".to_string(),
            role: Some("gateway".to_string()),
        }]
    );
    assert_eq!(
        parsed.targets,
        vec![ProfileTarget {
            name: "Staging API".to_string(),
            host: "staging-api.internal.company.com".to_string(),
            port: Some(443),
            kind: Some("service".to_string()),
        }]
    );
}

#[test]
fn missing_lists_default_to_empty() {
    let parsed = ProfileConfig::from_toml_str(
        r#"
[profile]
name = "default"
"#,
    )
    .expect("profile parses");

    assert!(parsed.networks.is_empty());
    assert!(parsed.hosts.is_empty());
    assert!(parsed.targets.is_empty());
}
```

- [x] **Step 2: Run failing test**

Run:

```bash
cargo test --test profile_model
```

Expected: FAIL because `lazyifconfig::profile` does not exist.

- [x] **Step 3: Add dependency and module exports**

In `Cargo.toml`, add:

```toml
toml = "0.8"
```

In `src/lib.rs`, add:

```rust
pub mod profile;
```

Create `src/profile/mod.rs`:

```rust
pub mod model;

pub use model::{
    ProfileAutoDetect, ProfileConfig, ProfileDocument, ProfileHost, ProfileNetwork, ProfileTarget,
};
```

- [x] **Step 4: Implement TOML model**

Create `src/profile/model.rs`:

```rust
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
```

- [x] **Step 5: Run test**

Run:

```bash
cargo test --test profile_model
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/profile/mod.rs src/profile/model.rs tests/profile_model.rs
git commit -m "feat: add profile toml model"
```

## Task 2: Profile Matching and Labels

**Files:**
- Modify: `src/profile/mod.rs`
- Create: `src/profile/match.rs`
- Create: `src/profile/join.rs`
- Test: `tests/profile_matching.rs`

- [ ] **Step 1: Add failing matching tests**

Create `tests/profile_matching.rs`:

```rust
use lazyifconfig::profile::{label_ip, suggest_profile, ProfileConfig, ProfileSuggestionInput};
use lazyifconfig::model::RouteEntry;

fn profile(name: &str, cidr: &str, gateway: &str) -> ProfileConfig {
    ProfileConfig::from_toml_str(&format!(
        r#"
[profile]
name = "{name}"

[[networks]]
cidr = "{cidr}"
name = "{name} LAN"
kind = "lan"

[[hosts]]
ip = "{gateway}"
name = "{name}-gateway"
role = "gateway"
"#
    ))
    .unwrap()
}

#[test]
fn label_ip_prefers_exact_host_then_network() {
    let office = profile("office", "10.20.0.0/16", "10.20.1.1");

    assert_eq!(
        label_ip("10.20.1.1", &office).unwrap().display,
        "office-gateway"
    );
    assert_eq!(
        label_ip("10.20.4.82", &office).unwrap().display,
        "office LAN"
    );
    assert!(label_ip("8.8.8.8", &office).is_none());
}

#[test]
fn suggestion_scores_fast_local_signals() {
    let office = profile("office", "10.20.0.0/16", "10.20.1.1");
    let home = profile("home", "192.168.0.0/24", "192.168.0.1");

    let input = ProfileSuggestionInput {
        interface_ips: vec!["10.20.4.82".to_string()],
        gateways: vec!["10.20.1.1".to_string()],
        routes: vec![RouteEntry::new("10.30.0.0/16", "10.20.1.1", "en0")],
    };

    let suggestion = suggest_profile(&[home, office], &input).expect("suggestion exists");

    assert_eq!(suggestion.profile_name, "office");
    assert!(suggestion.score >= 80);
    assert!(suggestion.reasons.iter().any(|reason| reason.contains("network")));
    assert!(suggestion.reasons.iter().any(|reason| reason.contains("gateway")));
}
```

- [ ] **Step 2: Run failing test**

Run:

```bash
cargo test --test profile_matching
```

Expected: FAIL because matching API does not exist.

- [ ] **Step 3: Export matching modules**

Modify `src/profile/mod.rs`:

```rust
pub mod join;
pub mod r#match;
pub mod model;

pub use join::{label_ip, ProfileIpLabel};
pub use model::{
    ProfileAutoDetect, ProfileConfig, ProfileDocument, ProfileHost, ProfileNetwork, ProfileTarget,
};
pub use r#match::{suggest_profile, ProfileSuggestion, ProfileSuggestionInput};
```

- [ ] **Step 4: Implement CIDR and suggestion logic**

Create `src/profile/match.rs`:

```rust
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
        if profile.networks.iter().any(|network| ip_in_cidr(ip, &network.cidr)) {
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
```

- [ ] **Step 5: Implement label helper**

Create `src/profile/join.rs`:

```rust
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
```

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test --test profile_model --test profile_matching
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/profile/mod.rs src/profile/match.rs src/profile/join.rs tests/profile_matching.rs
git commit -m "feat: match profile labels"
```

## Task 3: Profile Store and Active Profile State

**Files:**
- Modify: `src/profile/mod.rs`
- Create: `src/profile/store.rs`
- Modify: `src/app.rs`
- Modify: `src/main.rs`
- Test: `tests/profile_store.rs`
- Test: `tests/app_state.rs`

- [ ] **Step 1: Add failing store tests**

Create `tests/profile_store.rs`:

```rust
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
```

Append to `tests/app_state.rs`:

```rust
#[test]
fn app_defaults_to_default_profile_name() {
    let app = App::default();

    assert_eq!(app.active_profile_name(), "default");
    assert!(app.active_profile().is_none());
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cargo test --test profile_store app_defaults_to_default_profile_name
```

Expected: FAIL because store API and app profile state do not exist.

- [ ] **Step 3: Export store module**

Modify `src/profile/mod.rs`:

```rust
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
    config_path_for_base, default_profile_path_for_base, load_profile_from_path,
    profile_path_for_base,
};
```

- [ ] **Step 4: Implement profile path/store helpers**

Create `src/profile/store.rs`:

```rust
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
```

- [ ] **Step 5: Add app profile state**

Modify `src/app.rs`: import `ProfileConfig`, add fields to `App`, default them, and add helpers.

```rust
use crate::profile::ProfileConfig;
```

Add fields:

```rust
pub active_profile_name: String,
pub active_profile: Option<ProfileConfig>,
pub profile_warning: Option<String>,
```

Default values:

```rust
active_profile_name: "default".to_string(),
active_profile: None,
profile_warning: None,
```

Add methods:

```rust
pub fn active_profile_name(&self) -> &str {
    self.active_profile_name.as_str()
}

pub fn active_profile(&self) -> Option<&ProfileConfig> {
    self.active_profile.as_ref()
}

pub fn set_active_profile(&mut self, name: impl Into<String>, profile: Option<ProfileConfig>) {
    self.active_profile_name = name.into();
    self.active_profile = profile;
    self.profile_warning = None;
}

pub fn set_profile_warning(&mut self, warning: impl Into<String>) {
    self.profile_warning = Some(warning.into());
}
```

- [ ] **Step 6: Wire CLI profile name only**

Modify `src/main.rs` before TUI init:

```rust
let mut profile_override: Option<String> = None;
let mut remaining_args = Vec::new();
let mut iter = cli_args.into_iter();
while let Some(arg) = iter.next() {
    if arg == "--profile" {
        if let Some(value) = iter.next() {
            profile_override = Some(value);
        }
    } else {
        remaining_args.push(arg);
    }
}
let cli_args = remaining_args;
```

After `let mut app = App::default();`:

```rust
if let Some(profile_name) = profile_override {
    app.active_profile_name = profile_name;
}
```

- [ ] **Step 7: Run tests**

Run:

```bash
cargo test --test profile_store --test app_state
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/profile/mod.rs src/profile/store.rs src/app.rs src/main.rs tests/profile_store.rs tests/app_state.rs
git commit -m "feat: track active profile state"
```

## Task 4: Show Active Profile in UI

**Files:**
- Modify: `src/ui.rs`
- Test: `tests/app_state.rs`

- [ ] **Step 1: Add display helper test**

Append to `tests/app_state.rs`:

```rust
#[test]
fn app_profile_status_text_includes_warning_when_present() {
    let mut app = App::default();
    app.set_profile_warning("profile file not found");

    assert_eq!(
        app.profile_status_text(),
        "Profile: default (profile file not found)"
    );
}
```

- [ ] **Step 2: Run failing test**

Run:

```bash
cargo test --test app_state app_profile_status_text_includes_warning_when_present
```

Expected: FAIL because `profile_status_text` does not exist.

- [ ] **Step 3: Add app helper**

Modify `src/app.rs`:

```rust
pub fn profile_status_text(&self) -> String {
    if let Some(warning) = &self.profile_warning {
        format!("Profile: {} ({warning})", self.active_profile_name)
    } else {
        format!("Profile: {}", self.active_profile_name)
    }
}
```

- [ ] **Step 4: Render profile in header**

Modify `src/ui.rs` `header_line(app)` after OS label:

```rust
spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
spans.push(Span::styled(
    app.profile_status_text(),
    Style::default().fg(if app.profile_warning.is_some() {
        Color::Yellow
    } else {
        Color::LightCyan
    }),
));
```

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test --test app_state
cargo test
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/app.rs src/ui.rs tests/app_state.rs
git commit -m "feat: show active profile"
```

## Task 5: Join Profile Labels into Existing Views

**Files:**
- Modify: `src/app.rs`
- Modify: `src/ui/tables.rs`
- Modify: `src/ui/details.rs`
- Test: `tests/app_state.rs`

- [ ] **Step 1: Add app label helper tests**

Append to `tests/app_state.rs`:

```rust
#[test]
fn app_labels_ip_with_active_profile() {
    let mut app = App::default();
    let profile = lazyifconfig::profile::ProfileConfig::from_toml_str(
        r#"
[profile]
name = "office"

[[networks]]
cidr = "10.20.0.0/16"
name = "Office LAN"
kind = "lan"

[[hosts]]
ip = "10.20.1.1"
name = "office-gateway"
role = "gateway"
"#,
    )
    .unwrap();

    app.set_active_profile("office", Some(profile));

    assert_eq!(app.profile_label_for_ip("10.20.1.1"), Some("office-gateway".to_string()));
    assert_eq!(app.profile_label_for_ip("10.20.4.82"), Some("Office LAN".to_string()));
    assert_eq!(app.profile_label_for_ip("8.8.8.8"), None);
}
```

- [ ] **Step 2: Run failing test**

Run:

```bash
cargo test --test app_state app_labels_ip_with_active_profile
```

Expected: FAIL because `profile_label_for_ip` does not exist.

- [ ] **Step 3: Add app label helpers**

Modify `src/app.rs`:

```rust
pub fn profile_label_for_ip(&self, ip: &str) -> Option<String> {
    let profile = self.active_profile.as_ref()?;
    crate::profile::label_ip(ip, profile).map(|label| label.display)
}

pub fn profile_ip_display(&self, ip: &str) -> String {
    if let Some(label) = self.profile_label_for_ip(ip) {
        format!("{ip} {label}")
    } else {
        ip.to_string()
    }
}
```

- [ ] **Step 4: Use labels in connection table**

Modify `src/ui/tables.rs` in `render_connections_table` row creation:

```rust
highlighted_filter_cell(app.profile_ip_display(local_ip), &app.connection_filter),
highlighted_filter_cell(local_port.clone(), &app.connection_filter),
highlighted_filter_cell(app.profile_ip_display(foreign_ip), &app.connection_filter),
```

Increase local/foreign IP constraints:

```rust
Constraint::Length(18),
Constraint::Length(5),
Constraint::Length(18),
```

- [ ] **Step 5: Use labels in details**

Modify `src/ui/details.rs` `connection_summary_lines` signature to accept already formatted IPs, or add profile-aware formatting at call site where `connection_summary_lines` is called. Keep raw `foreign_ip` for WHOIS action logic.

Expected visible strings:

```text
Local IP:          10.20.4.82 Office LAN
Foreign IP:        10.20.1.1 office-gateway
```

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test --test app_state
cargo test
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/app.rs src/ui/tables.rs src/ui/details.rs tests/app_state.rs
git commit -m "feat: join profile labels into views"
```

## Task 6: Profile Switcher Shell

**Files:**
- Modify: `src/app.rs`
- Modify: `src/main.rs`
- Modify: `src/ui/overlays.rs`
- Modify: `src/ui.rs`
- Test: `tests/app_state.rs`

- [ ] **Step 1: Add switcher state tests**

Append to `tests/app_state.rs`:

```rust
#[test]
fn profile_switcher_opens_and_selects_next_profile() {
    let mut app = App::default();
    app.available_profile_names = vec![
        "default".to_string(),
        "home".to_string(),
        "office".to_string(),
    ];

    app.open_profile_switcher();
    assert!(app.profile_switcher.active);
    assert_eq!(app.profile_switcher.selected_index, 0);

    app.profile_switcher_next();
    app.profile_switcher_next();
    assert_eq!(app.selected_profile_switcher_name(), Some("office"));
}
```

- [ ] **Step 2: Run failing test**

Run:

```bash
cargo test --test app_state profile_switcher_opens_and_selects_next_profile
```

Expected: FAIL because switcher state does not exist.

- [ ] **Step 3: Add switcher state**

Modify `src/app.rs`:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileSwitcherState {
    pub active: bool,
    pub selected_index: usize,
}
```

Add fields:

```rust
pub available_profile_names: Vec<String>,
pub profile_switcher: ProfileSwitcherState,
```

Default values:

```rust
available_profile_names: vec!["default".to_string()],
profile_switcher: ProfileSwitcherState::default(),
```

Add methods:

```rust
pub fn open_profile_switcher(&mut self) {
    self.profile_switcher.active = true;
    self.profile_switcher.selected_index = self
        .available_profile_names
        .iter()
        .position(|name| name == &self.active_profile_name)
        .unwrap_or(0);
}

pub fn close_profile_switcher(&mut self) {
    self.profile_switcher.active = false;
}

pub fn profile_switcher_next(&mut self) {
    if !self.available_profile_names.is_empty() {
        self.profile_switcher.selected_index =
            (self.profile_switcher.selected_index + 1) % self.available_profile_names.len();
    }
}

pub fn profile_switcher_previous(&mut self) {
    if !self.available_profile_names.is_empty() {
        self.profile_switcher.selected_index =
            (self.profile_switcher.selected_index + self.available_profile_names.len() - 1)
                % self.available_profile_names.len();
    }
}

pub fn selected_profile_switcher_name(&self) -> Option<&str> {
    self.available_profile_names
        .get(self.profile_switcher.selected_index)
        .map(String::as_str)
}
```

- [ ] **Step 4: Add key handling**

Modify `src/main.rs` main key handling:

```rust
if app.profile_switcher.active {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.close_profile_switcher(),
        KeyCode::Char('j') | KeyCode::Down => app.profile_switcher_next(),
        KeyCode::Char('k') | KeyCode::Up => app.profile_switcher_previous(),
        KeyCode::Enter => {
            if let Some(name) = app.selected_profile_switcher_name().map(str::to_string) {
                app.active_profile_name = name;
            }
            app.close_profile_switcher();
        }
        _ => {}
    }
    continue;
}
```

Add global key:

```rust
KeyCode::Char('P') => app.open_profile_switcher(),
```

- [ ] **Step 5: Render switcher overlay**

Modify `src/ui/overlays.rs` to add `draw_profile_switcher(frame, app, area)` that renders profile names and selected row.

Modify `src/ui.rs` `draw` after main layout render:

```rust
if app.profile_switcher.active {
    overlays::draw_profile_switcher(f, app, size);
}
```

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test --test app_state
cargo test
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/app.rs src/main.rs src/ui.rs src/ui/overlays.rs tests/app_state.rs
git commit -m "feat: add profile switcher"
```

## Task 7: Fast Profile Detection

**Files:**
- Modify: `src/app.rs`
- Modify: `src/profile/match.rs`
- Modify: `src/ui.rs`
- Test: `tests/profile_matching.rs`
- Test: `tests/app_state.rs`

- [ ] **Step 1: Add detection tests**

Append to `tests/app_state.rs`:

```rust
#[test]
fn app_builds_profile_suggestion_input_from_snapshot() {
    let mut app = App::default();
    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![interface_with_stats("en0", Some("10.20.4.82"), None)],
    ));

    let input = app.profile_suggestion_input().expect("snapshot input exists");

    assert_eq!(input.interface_ips, vec!["10.20.4.82"]);
}
```

- [ ] **Step 2: Run failing test**

Run:

```bash
cargo test --test app_state app_builds_profile_suggestion_input_from_snapshot
```

Expected: FAIL because helper does not exist.

- [ ] **Step 3: Add app suggestion state and helper**

Modify `src/app.rs`:

```rust
pub profile_suggestion: Option<crate::profile::ProfileSuggestion>,
pub loaded_profiles: Vec<ProfileConfig>,
```

Default:

```rust
profile_suggestion: None,
loaded_profiles: Vec::new(),
```

Helper:

```rust
pub fn profile_suggestion_input(&self) -> Option<crate::profile::ProfileSuggestionInput> {
    let snapshot = self.current_snapshot.as_ref()?;
    let interface_ips = snapshot
        .interfaces
        .iter()
        .flat_map(|interface| interface.ipv4.iter().map(|addr| addr.value.clone()))
        .collect();
    let gateways = snapshot
        .interfaces
        .iter()
        .flat_map(|interface| interface.ipv4.iter().filter_map(|addr| addr.gateway.clone()))
        .collect();
    Some(crate::profile::ProfileSuggestionInput {
        interface_ips,
        gateways,
        routes: snapshot.routes.clone(),
    })
}

pub fn update_profile_suggestion(&mut self) {
    if self.loaded_profiles.is_empty() {
        self.profile_suggestion = None;
        return;
    }
    self.profile_suggestion = self
        .profile_suggestion_input()
        .and_then(|input| crate::profile::suggest_profile(&self.loaded_profiles, &input));
}
```

Call `self.update_profile_suggestion();` at end of `replace_snapshot`.

- [ ] **Step 4: Show suggestion in profile status**

Modify `profile_status_text`:

```rust
if let Some(suggestion) = &self.profile_suggestion {
    if suggestion.profile_name != self.active_profile_name {
        return format!(
            "Profile: {} | Suggested: {}",
            self.active_profile_name, suggestion.profile_name
        );
    }
}
```

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test --test app_state --test profile_matching
cargo test
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/app.rs src/profile/match.rs src/ui.rs tests/app_state.rs tests/profile_matching.rs
git commit -m "feat: suggest profile from local network"
```

## Task 8: Profile Editor MVP

**Files:**
- Modify: `src/app.rs`
- Modify: `src/main.rs`
- Modify: `src/ui/overlays.rs`
- Modify: `src/profile/store.rs`
- Test: `tests/app_state.rs`
- Test: `tests/profile_store.rs`

- [ ] **Step 1: Add editor state tests**

Append to `tests/app_state.rs`:

```rust
#[test]
fn profile_editor_adds_detected_network_draft() {
    let mut app = App::default();
    app.open_profile_editor();
    app.profile_editor_add_network("10.20.0.0/16", "Office LAN", "lan");

    assert_eq!(app.profile_editor.networks.len(), 1);
    assert_eq!(app.profile_editor.networks[0].cidr, "10.20.0.0/16");
    assert_eq!(app.profile_editor.networks[0].name, "Office LAN");
}
```

- [ ] **Step 2: Run failing test**

Run:

```bash
cargo test --test app_state profile_editor_adds_detected_network_draft
```

Expected: FAIL because editor state does not exist.

- [ ] **Step 3: Add editor state**

Modify `src/app.rs`:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileEditorState {
    pub active: bool,
    pub networks: Vec<crate::profile::ProfileNetwork>,
    pub hosts: Vec<crate::profile::ProfileHost>,
    pub targets: Vec<crate::profile::ProfileTarget>,
    pub selected_section: usize,
    pub selected_index: usize,
}
```

Add field/default:

```rust
pub profile_editor: ProfileEditorState,
```

```rust
profile_editor: ProfileEditorState::default(),
```

Add methods:

```rust
pub fn open_profile_editor(&mut self) {
    self.profile_editor.active = true;
    if let Some(profile) = &self.active_profile {
        self.profile_editor.networks = profile.networks.clone();
        self.profile_editor.hosts = profile.hosts.clone();
        self.profile_editor.targets = profile.targets.clone();
    }
}

pub fn close_profile_editor(&mut self) {
    self.profile_editor.active = false;
}

pub fn profile_editor_add_network(&mut self, cidr: &str, name: &str, kind: &str) {
    self.profile_editor.networks.push(crate::profile::ProfileNetwork {
        cidr: cidr.to_string(),
        name: name.to_string(),
        kind: Some(kind.to_string()),
    });
}
```

- [ ] **Step 4: Add switcher key to editor**

Modify switcher key handling in `src/main.rs`:

```rust
KeyCode::Char('e') => {
    app.close_profile_switcher();
    app.open_profile_editor();
}
```

- [ ] **Step 5: Render editor read-only MVP**

Modify `src/ui/overlays.rs` to draw Networks, Hosts, Targets lists and footer:

```text
Profile Editor: office
a add | s save | Esc close
```

First version may show lists and allow close only. Add/edit text input comes next.

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test --test app_state
cargo test
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/app.rs src/main.rs src/ui/overlays.rs tests/app_state.rs
git commit -m "feat: add profile editor shell"
```

## Task 9: Local Doctor Model and First Checks

**Files:**
- Modify: `src/lib.rs`
- Create: `src/doctor/mod.rs`
- Create: `src/doctor/model.rs`
- Create: `src/doctor/checks.rs`
- Modify: `src/app.rs`
- Test: `tests/doctor.rs`

- [ ] **Step 1: Add failing doctor tests**

Create `tests/doctor.rs`:

```rust
use lazyifconfig::doctor::{run_fast_doctor_checks, DoctorSeverity};
use lazyifconfig::model::{
    InterfaceAddress, InterfaceStatus, InterfaceType, NetworkInterface, NetworkKind,
    NetworkSnapshot, RouteEntry,
};
use lazyifconfig::profile::ProfileConfig;

#[test]
fn doctor_reports_profile_network_present() {
    let profile = ProfileConfig::from_toml_str(
        r#"
[profile]
name = "office"

[[networks]]
cidr = "10.20.0.0/16"
name = "Office LAN"
kind = "lan"
"#,
    )
    .unwrap();

    let snapshot = NetworkSnapshot {
        interfaces: vec![NetworkInterface {
            name: "en0".to_string(),
            network_kind: NetworkKind::Lan,
            interface_type: InterfaceType::WifiOrEthernet,
            status: InterfaceStatus::Up,
            ipv4: vec![InterfaceAddress {
                value: "10.20.4.82".to_string(),
                prefix_len: Some(16),
                gateway: Some("10.20.1.1".to_string()),
            }],
            ipv6: vec![],
            mac_address: None,
            mtu: None,
            stats: None,
        }],
        connections: vec![],
        listening_ports: vec![],
        routes: vec![RouteEntry::new("default", "10.20.1.1", "en0")],
        captured_at_secs: 1,
    };

    let checks = run_fast_doctor_checks(&snapshot, Some(&profile));

    assert!(checks.iter().any(|check| {
        check.severity == DoctorSeverity::Ok && check.title == "Office LAN detected"
    }));
}
```

- [ ] **Step 2: Run failing test**

Run:

```bash
cargo test --test doctor
```

Expected: FAIL because doctor module does not exist.

- [ ] **Step 3: Add doctor module**

Modify `src/lib.rs`:

```rust
pub mod doctor;
```

Create `src/doctor/mod.rs`:

```rust
pub mod checks;
pub mod model;

pub use checks::run_fast_doctor_checks;
pub use model::{DoctorCheck, DoctorSeverity};
```

Create `src/doctor/model.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoctorSeverity {
    Ok,
    Warn,
    Fail,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoctorCheck {
    pub severity: DoctorSeverity,
    pub title: String,
    pub detail: String,
    pub next_step: String,
}
```

Create `src/doctor/checks.rs`:

```rust
use crate::doctor::{DoctorCheck, DoctorSeverity};
use crate::model::NetworkSnapshot;
use crate::profile::{label_ip, ProfileConfig};

pub fn run_fast_doctor_checks(
    snapshot: &NetworkSnapshot,
    profile: Option<&ProfileConfig>,
) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();

    if let Some(profile) = profile {
        for network in &profile.networks {
            let found = snapshot.interfaces.iter().any(|interface| {
                interface
                    .ipv4
                    .iter()
                    .any(|addr| label_ip(&addr.value, profile).is_some_and(|label| label.display == network.name))
            });

            if found {
                checks.push(DoctorCheck {
                    severity: DoctorSeverity::Ok,
                    title: format!("{} detected", network.name),
                    detail: format!("Current interface IP matches {}", network.cidr),
                    next_step: "No action needed.".to_string(),
                });
            }
        }
    }

    let has_default_route = snapshot.routes.iter().any(|route| route.destination == "default");
    checks.push(DoctorCheck {
        severity: if has_default_route {
            DoctorSeverity::Ok
        } else {
            DoctorSeverity::Fail
        },
        title: "Default route".to_string(),
        detail: if has_default_route {
            "Default route exists.".to_string()
        } else {
            "No default route found.".to_string()
        },
        next_step: if has_default_route {
            "No action needed.".to_string()
        } else {
            "Check gateway, Wi-Fi/Ethernet, or VPN route configuration.".to_string()
        },
    });

    checks
}
```

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test --test doctor
cargo test
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/doctor/mod.rs src/doctor/model.rs src/doctor/checks.rs tests/doctor.rs
git commit -m "feat: add local doctor checks"
```

## Task 10: Remote Doctor Report Import Shell

**Files:**
- Modify: `src/lib.rs`
- Create: `src/remote/mod.rs`
- Create: `src/remote/report.rs`
- Create: `src/remote/windows.rs`
- Modify: `src/main.rs`
- Test: `tests/remote.rs`

- [ ] **Step 1: Add report parsing tests**

Create `tests/remote.rs`:

```rust
use lazyifconfig::remote::{
    parse_remote_report, windows_collector_script, RemotePlatform, RemoteReport,
};

#[test]
fn parses_remote_report_metadata_and_outputs() {
    let input = r#"
{
  "platform": "windows",
  "hostname": "dev-laptop",
  "captured_at": "2026-06-12T10:00:00Z",
  "commands": [
    {
      "name": "ipconfig_all",
      "command": "ipconfig /all",
      "stdout": "Windows IP Configuration",
      "stderr": "",
      "exit_code": 0
    }
  ]
}
"#;

    let report = parse_remote_report(input).expect("report parses");

    assert_eq!(
        report,
        RemoteReport {
            platform: RemotePlatform::Windows,
            hostname: Some("dev-laptop".to_string()),
            captured_at: Some("2026-06-12T10:00:00Z".to_string()),
            commands: vec![lazyifconfig::remote::RemoteCommandOutput {
                name: "ipconfig_all".to_string(),
                command: "ipconfig /all".to_string(),
                stdout: "Windows IP Configuration".to_string(),
                stderr: String::new(),
                exit_code: Some(0),
            }],
        }
    );
}

#[test]
fn windows_collector_script_uses_json_friendly_commands() {
    let script = windows_collector_script();

    assert!(script.contains("Get-NetIPConfiguration"));
    assert!(script.contains("Get-NetRoute"));
    assert!(script.contains("Get-NetTCPConnection"));
    assert!(script.contains("ConvertTo-Json"));
}
```

- [ ] **Step 2: Run failing test**

Run:

```bash
cargo test --test remote
```

Expected: FAIL because remote module does not exist.

- [ ] **Step 3: Add remote module**

Modify `src/lib.rs`:

```rust
pub mod remote;
```

Create `src/remote/mod.rs`:

```rust
pub mod report;
pub mod windows;

pub use report::{parse_remote_report, RemoteCommandOutput, RemotePlatform, RemoteReport};
pub use windows::windows_collector_script;
```

Create `src/remote/report.rs`:

```rust
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct RemoteReport {
    pub platform: RemotePlatform,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub captured_at: Option<String>,
    #[serde(default)]
    pub commands: Vec<RemoteCommandOutput>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RemotePlatform {
    Windows,
    Macos,
    Linux,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct RemoteCommandOutput {
    pub name: String,
    pub command: String,
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
}

pub fn parse_remote_report(input: &str) -> Result<RemoteReport, String> {
    serde_json::from_str(input).map_err(|error| error.to_string())
}
```

Create `src/remote/windows.rs`:

```rust
pub fn windows_collector_script() -> &'static str {
    r#"
$ErrorActionPreference = "Continue"
$commands = @()

function Add-CommandResult($name, $command, $stdout, $stderr, $exitCode) {
  $script:commands += [ordered]@{
    name = $name
    command = $command
    stdout = $stdout
    stderr = $stderr
    exit_code = $exitCode
  }
}

try {
  Add-CommandResult "ipconfig_all" "ipconfig /all" (ipconfig /all | Out-String) "" 0
} catch {
  Add-CommandResult "ipconfig_all" "ipconfig /all" "" ($_ | Out-String) 1
}

try {
  Add-CommandResult "net_ip_configuration" "Get-NetIPConfiguration | ConvertTo-Json -Depth 6" (Get-NetIPConfiguration | ConvertTo-Json -Depth 6) "" 0
} catch {
  Add-CommandResult "net_ip_configuration" "Get-NetIPConfiguration | ConvertTo-Json -Depth 6" "" ($_ | Out-String) 1
}

try {
  Add-CommandResult "net_route" "Get-NetRoute | ConvertTo-Json -Depth 6" (Get-NetRoute | ConvertTo-Json -Depth 6) "" 0
} catch {
  Add-CommandResult "net_route" "Get-NetRoute | ConvertTo-Json -Depth 6" "" ($_ | Out-String) 1
}

try {
  Add-CommandResult "net_tcp_connection" "Get-NetTCPConnection | ConvertTo-Json -Depth 6" (Get-NetTCPConnection | ConvertTo-Json -Depth 6) "" 0
} catch {
  Add-CommandResult "net_tcp_connection" "Get-NetTCPConnection | ConvertTo-Json -Depth 6" "" ($_ | Out-String) 1
}

[ordered]@{
  platform = "windows"
  hostname = $env:COMPUTERNAME
  captured_at = (Get-Date).ToUniversalTime().ToString("o")
  commands = $commands
} | ConvertTo-Json -Depth 8
"#
}
```

- [ ] **Step 4: Add CLI report shape only**

Modify `src/main.rs` args parsing to accept:

```text
doctor --from-report lazyifconfig-report.json
doctor --profile office --from-report lazyifconfig-report.json
doctor --print-windows-collector
```

First shell behavior:

```text
doctor --print-windows-collector
```

prints the PowerShell collector script.

```text
doctor --profile office --from-report lazyifconfig-report.json
```

loads and parses the report, then prints:

```text
Remote Doctor report loaded: windows dev-laptop, profile office
```

Do not run Doctor checks from the imported report yet in this task.

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test --test remote
cargo test
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/lib.rs src/remote/mod.rs src/remote/report.rs src/remote/windows.rs src/main.rs tests/remote.rs
git commit -m "feat: add remote doctor report import shell"
```

## Verification Gate Before Each Slice Completion

Run:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Expected:

```text
PASS
```

If `cargo clippy` reports pre-existing warnings unrelated to current slice, record exact warning and fix only if it blocks the slice.

## Execution Order

Start with Task 1 only. After Task 1 passes, review diff, then continue Task 2. Do not start Doctor before Profile Core + visible active profile work is complete.

Recommended first milestone:

- Task 1
- Task 2
- Task 3
- Task 4
- Task 5

This produces useful Profile Core before Switcher/Detection/Editor.
