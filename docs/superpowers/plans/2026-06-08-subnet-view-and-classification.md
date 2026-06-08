# 서브넷 그룹 뷰 및 네트워크 분류 기능 구현 계획서

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `lazyifconfig`에 네트워크 주소 대역 기반의 서브넷 그룹 뷰와 인터페이스 역할에 맞는 네트워크 분류(LAN, VPN, Container, Public 등) 기능을 통합 구현하여 TUI 사용성을 개선합니다.

**Architecture:**
- `src/model.rs`에 `NetworkKind`, `Subnet` 모델 및 `Subnet` 전용 정렬 로직(`Ord`)을 정의하고 `InterfaceAddress`에 `prefix_len` 필드를 통합합니다.
- `src/collector/interface.rs`에서 `ifconfig` 출력 속 `netmask`/`prefixlen`을 파싱하고, 우선순위에 근거하여 `NetworkKind` 역할을 Heuristic 분류합니다.
- `src/app.rs`에 `NavigationItem` 추상 레이어를 설계하고 뷰 모드(인터페이스/네트워크) 전환 시 선택 커서를 동적으로 보존하도록 탐색 로직을 리팩토링합니다.
- `src/ui.rs` 및 `src/main.rs`에서 단축키 `i`/`n` 뷰 매핑 및 서브넷 헤더 정보 출력과 들여쓰기 렌더링을 추가합니다.

**Tech Stack:** Rust, Ratatui, Crossterm, Tokio.

---

## Proposed Changes

### 1. Model Layer
- Modify: [src/model.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/model.rs)
  - `NetworkKind` enum 추가 및 `InterfaceAddress`, `NetworkInterface` 필드 수정.
  - `Subnet` enum 추가 및 `Ord`/`PartialOrd` 커스텀 구현.

### 2. Collector Layer
- Modify: [src/collector/interface.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/collector/interface.rs)
  - `parse_interfaces`를 확장하여 netmask, prefixlen 파싱 및 인터페이스 분류(`classify_interface`) 수행.
  - IP 주소에 따른 서브넷 연산 헬퍼 추가.

### 3. App State Layer
- Modify: [src/app.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/app.rs)
  - `ViewMode`, `NavigationItem` 정의 및 `App` 내 `navigation_items` 목록 관리.
  - 뷰 모드 토글(`set_view_mode`), 선택 위치 복원(`restore_selection`) 수정.
  - `j/k` 이동을 `navigation_items` 목록 인덱스 탐색으로 변경.

### 4. UI Layer
- Modify: [src/ui.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/ui.rs)
  - 좌측 리스트에 네트워크 분류명 노출 및 들여쓰기 출력.
  - 우측 디테일 패널에 선택된 서브넷 정보 또는 분류 필드 노출.
  - 상태 표시줄 힌트 추가.

### 5. Main Loop
- Modify: [src/main.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/main.rs)
  - 키 바인딩(`i`, `n`) 처리 추가.

---

## Tasks

### Task 1: 데이터 모델 정의 및 Subnet 수동 정렬 구현

**Files:**
- Modify: `src/model.rs`

- [ ] **Step 1: 실패하는 테스트 작성**
  
  `tests/subnet_model.rs` 파일을 새로 만들거나 `src/model.rs` 하단 테스트 블록에 서브넷 비교 정렬을 체크하는 테스트 코드를 작성합니다:
  ```rust
  #[cfg(test)]
  mod model_tests {
      use super::*;
      use std::net::{Ipv4Addr, Ipv6Addr};

      #[test]
      fn test_subnet_sorting_order() {
          let ip4_1 = Subnet::Ipv4 { network: Ipv4Addr::new(10, 0, 0, 0), prefix_len: 8 };
          let ip4_2 = Subnet::Ipv4 { network: Ipv4Addr::new(192, 168, 0, 0), prefix_len: 24 };
          let ip6_1 = Subnet::Ipv6 { network: Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0), prefix_len: 64 };
          let unassigned = Subnet::Unassigned;

          let mut subnets = vec![unassigned.clone(), ip6_1.clone(), ip4_2.clone(), ip4_1.clone()];
          subnets.sort();

          // 정렬 기대 순서: IPv4 -> IPv6 -> Unassigned
          assert_eq!(subnets[0], ip4_1);
          assert_eq!(subnets[1], ip4_2);
          assert_eq!(subnets[2], ip6_1);
          assert_eq!(subnets[3], unassigned);
      }
  }
  ```

- [ ] **Step 2: 테스트를 실행하여 실패하는지 확인**
  
  Run: `cargo test --lib model::model_tests::test_subnet_sorting_order`
  Expected: 컴파일 에러 (Subnet, NetworkKind 등이 선언되지 않음)

- [ ] **Step 3: model.rs 구현 작성**
  
  [src/model.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/model.rs) 내용을 다음과 같이 수정하여 모델 정의 및 수동 `Ord` 정렬을 구현합니다.
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum NetworkKind {
      Loopback,
      Lan,
      Vpn,
      Container,
      LinkLocal,
      Public,
      Unknown,
  }

  impl NetworkKind {
      pub fn as_str(&self) -> &'static str {
          match self {
              NetworkKind::Loopback => "LOOPBACK",
              NetworkKind::Lan => "LAN",
              NetworkKind::Vpn => "VPN",
              NetworkKind::Container => "CONTAINER",
              NetworkKind::LinkLocal => "LINK LOCAL",
              NetworkKind::Public => "PUBLIC",
              NetworkKind::Unknown => "UNKNOWN",
          }
      }
  }

  #[derive(Clone, Debug, PartialEq, Eq)]
  pub enum InterfaceType {
      Vpn,
      Loopback,
      Bridge,
      AirDrop,
      WifiOrEthernet,
      Unknown,
  }

  #[derive(Clone, Debug, PartialEq, Eq)]
  pub enum InterfaceStatus {
      Up,
      Down,
  }

  #[derive(Clone, Debug, PartialEq, Eq)]
  pub struct InterfaceAddress {
      pub value: String,
      pub prefix_len: Option<u8>,
  }

  impl InterfaceAddress {
      pub fn new(value: &str) -> Self {
          Self {
              value: value.to_string(),
              prefix_len: None,
          }
      }
  }

  #[derive(Clone, Debug, Default, PartialEq, Eq)]
  pub struct InterfaceStats {
      pub rx_bytes: u64,
      pub tx_bytes: u64,
      pub rx_packets: u64,
      pub tx_packets: u64,
  }

  #[derive(Clone, Debug, PartialEq, Eq)]
  pub struct NetworkInterface {
      pub name: String,
      pub network_kind: NetworkKind,
      pub interface_type: InterfaceType,
      pub status: InterfaceStatus,
      pub ipv4: Vec<InterfaceAddress>,
      pub ipv6: Vec<InterfaceAddress>,
      pub mac_address: Option<String>,
      pub mtu: Option<u32>,
      pub stats: Option<InterfaceStats>,
  }

  #[derive(Clone, Debug, Default, PartialEq, Eq)]
  pub struct NetworkSnapshot {
      pub interfaces: Vec<NetworkInterface>,
      pub captured_at_secs: u64,
  }

  #[derive(Clone, Debug, PartialEq, Eq)]
  pub struct NetworkEvent {
      pub message: String,
      pub captured_at_secs: u64,
  }

  impl NetworkEvent {
      pub fn new(message: String, captured_at_secs: u64) -> Self {
          Self {
              message,
              captured_at_secs,
          }
      }
  }

  #[derive(Clone, Debug, PartialEq, Eq, Hash)]
  pub enum Subnet {
      Ipv4 {
          network: std::net::Ipv4Addr,
          prefix_len: u8,
      },
      Ipv6 {
          network: std::net::Ipv6Addr,
          prefix_len: u8,
      },
      Unassigned,
  }

  impl Ord for Subnet {
      fn cmp(&self, other: &Self) -> std::cmp::Ordering {
          use std::cmp::Ordering;
          match (self, other) {
              (Subnet::Ipv4 { network: n1, prefix_len: p1 }, Subnet::Ipv4 { network: n2, prefix_len: p2 }) => {
                  n1.cmp(n2).then(p1.cmp(p2))
              }
              (Subnet::Ipv6 { network: n1, prefix_len: p1 }, Subnet::Ipv6 { network: n2, prefix_len: p2 }) => {
                  n1.cmp(n2).then(p1.cmp(p2))
              }
              (Subnet::Unassigned, Subnet::Unassigned) => Ordering::Equal,
              (Subnet::Ipv4 { .. }, _) => Ordering::Less,
              (_, Subnet::Ipv4 { .. }) => Ordering::Greater,
              (Subnet::Ipv6 { .. }, _) => Ordering::Less,
              (_, Subnet::Ipv6 { .. }) => Ordering::Greater,
          }
      }
  }

  impl PartialOrd for Subnet {
      fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
          Some(self.cmp(other))
      }
  }
  ```

- [ ] **Step 4: 테스트 실행하여 통과하는지 확인**
  
  Run: `cargo test`
  Expected: 모델 테스트 성공. 단, 기존 파서/앱코드 컴파일 중 에러가 발생할 수 있습니다.

- [ ] **Step 5: 커밋**
  
  ```bash
  git add src/model.rs
  git commit -m "feat: define NetworkKind, Subnet, and custom Ord implementations"
  ```

---

### Task 2: 파서 수정 및 네트워크 분류/서브넷 계산 연산 구현

**Files:**
- Modify: `src/collector/interface.rs`

- [ ] **Step 1: 실패하는 테스트 작성**
  
  `tests/parser_interface.rs`에 네트워크 분류 테스트 케이스를 추가합니다:
  ```rust
  #[test]
  fn test_network_classification_priority() {
      // utun은 사설 IP가 있어도 VPN으로 분류되어야 함
      let input = "utun4: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1500\n\tinet 10.8.0.2 netmask 0xffffff00";
      let parsed = parse_interfaces(input);
      assert_eq!(parsed[0].network_kind, lazyifconfig::model::NetworkKind::Vpn);

      // en0에 사설 IP가 있으면 LAN
      let input2 = "en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500\n\tinet 192.168.0.15 netmask 0xffffff00";
      let parsed2 = parse_interfaces(input2);
      assert_eq!(parsed2[0].network_kind, lazyifconfig::model::NetworkKind::Lan);
  }
  ```

- [ ] **Step 2: 테스트를 실행하여 실패하는지 확인**
  
  Run: `cargo test --test parser_interface test_network_classification_priority`
  Expected: 컴파일 에러 혹은 실패.

- [ ] **Step 3: 파서 및 서브넷/분류 구현**
  
  [src/collector/interface.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/collector/interface.rs)에 `classify_interface` 함수 및 서브넷 마스크 변환 헬퍼를 적용하고, `parse_interfaces`를 고도화합니다:
  ```rust
  use crate::model::{
      InterfaceAddress, InterfaceStatus, InterfaceType, NetworkInterface, NetworkKind,
  };

  pub fn parse_interfaces(input: &str) -> Vec<NetworkInterface> {
      let mut interfaces = Vec::new();
      let mut current: Option<NetworkInterface> = None;

      for line in input.lines() {
          if is_interface_header(line) {
              if let Some(mut interface) = current.take() {
                  interface.network_kind = classify_interface(&interface.name, &interface.ipv4, &interface.ipv6);
                  interfaces.push(interface);
              }

              let name = line.split(':').next().unwrap_or_default().to_string();
              current = Some(NetworkInterface {
                  interface_type: infer_interface_type(&name),
                  network_kind: NetworkKind::Unknown,
                  status: parse_header_status(line),
                  mtu: parse_mtu(line),
                  name,
                  ipv4: Vec::new(),
                  ipv6: Vec::new(),
                  mac_address: None,
                  stats: None,
              });
              continue;
          }

          let Some(interface) = current.as_mut() else {
              continue;
          };

          let trimmed = line.trim();

          if let Some(value) = trimmed.strip_prefix("ether ") {
              interface.mac_address = Some(value.to_string());
          } else if let Some(value) = trimmed.strip_prefix("inet6 ") {
              let parts: Vec<&str> = value.split_whitespace().collect();
              if !parts.is_empty() {
                  let address = parts[0].to_string();
                  let mut prefix_len = None;
                  if let Some(pos) = parts.iter().position(|&p| p == "prefixlen") {
                      if pos + 1 < parts.len() {
                          prefix_len = parts[pos + 1].parse::<u8>().ok();
                      }
                  }
                  interface.ipv6.push(InterfaceAddress {
                      value: address,
                      prefix_len,
                  });
              }
          } else if let Some(value) = trimmed.strip_prefix("inet ") {
              let parts: Vec<&str> = value.split_whitespace().collect();
              if !parts.is_empty() {
                  let address = parts[0].to_string();
                  let mut prefix_len = None;
                  if let Some(pos) = parts.iter().position(|&p| p == "netmask") {
                      if pos + 1 < parts.len() {
                          let hex_mask = parts[pos + 1];
                          prefix_len = parse_hex_netmask(hex_mask);
                      }
                  }
                  interface.ipv4.push(InterfaceAddress {
                      value: address,
                      prefix_len,
                  });
              }
          } else if trimmed == "status: active" {
              interface.status = InterfaceStatus::Up;
          } else if trimmed == "status: inactive" {
              interface.status = InterfaceStatus::Down;
          }
      }

      if let Some(mut interface) = current {
          interface.network_kind = classify_interface(&interface.name, &interface.ipv4, &interface.ipv6);
          interfaces.push(interface);
      }

      interfaces
  }

  fn parse_hex_netmask(hex_str: &str) -> Option<u8> {
      let hex_val = hex_str.strip_prefix("0x")?;
      let val = u32::from_str_radix(hex_val, 16).ok()?;
      Some(val.count_ones() as u8)
  }

  fn classify_interface(name: &str, ipv4: &[InterfaceAddress], ipv6: &[InterfaceAddress]) -> NetworkKind {
      // 1. Loopback
      if name == "lo0" || name.starts_with("lo") {
          return NetworkKind::Loopback;
      }
      for addr in ipv4 {
          if let Ok(ip) = addr.value.parse::<std::net::Ipv4Addr>() {
              if ip.is_loopback() {
                  return NetworkKind::Loopback;
              }
          }
      }
      for addr in ipv6 {
          if let Ok(ip) = addr.value.parse::<std::net::Ipv6Addr>() {
              if ip.is_loopback() {
                  return NetworkKind::Loopback;
              }
          }
      }

      // 2. VPN (utun, tun, tap, wg)
      if name.starts_with("utun") || name.starts_with("tun") || name.starts_with("tap") || name.starts_with("wg") {
          return NetworkKind::Vpn;
      }

      // 3. Container (docker, bridge, br-)
      if name.starts_with("docker") || name.starts_with("bridge") || name.starts_with("br-") {
          return NetworkKind::Container;
      }

      // 4. Link Local
      for addr in ipv4 {
          if let Ok(ip) = addr.value.parse::<std::net::Ipv4Addr>() {
              if ip.is_link_local() {
                  return NetworkKind::LinkLocal;
              }
          }
      }
      for addr in ipv6 {
          if let Ok(ip) = addr.value.parse::<std::net::Ipv6Addr>() {
              let octets = ip.octets();
              if octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80 {
                  return NetworkKind::LinkLocal;
              }
          }
      }

      // 5. Public / LAN
      let mut has_lan = false;
      let mut has_public = false;

      for addr in ipv4 {
          if let Ok(ip) = addr.value.parse::<std::net::Ipv4Addr>() {
              if ip.is_private() {
                  has_lan = true;
              } else if !ip.is_loopback() && !ip.is_link_local() {
                  has_public = true;
              }
          }
      }

      for addr in ipv6 {
          if let Ok(ip) = addr.value.parse::<std::net::Ipv6Addr>() {
              let octets = ip.octets();
              let is_unique_local = (octets[0] & 0xfe) == 0xfc;
              let is_loopback = ip.is_loopback();
              let is_link_local = octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80;
              let is_multicast = octets[0] == 0xff;
              let is_unspecified = ip.is_unspecified();

              if is_unique_local {
                  has_lan = true;
              } else if !is_loopback && !is_link_local && !is_multicast && !is_unspecified {
                  has_public = true;
              }
          }
      }

      if has_public {
          return NetworkKind::Public;
      }
      if has_lan {
          return NetworkKind::Lan;
      }

      NetworkKind::Unknown
  }

  fn is_interface_header(line: &str) -> bool {
      !line.starts_with(' ') && !line.starts_with('\t') && line.contains(':')
  }

  fn parse_header_status(line: &str) -> InterfaceStatus {
      if line.contains("<UP") || line.contains(",UP,") || line.contains(",UP>") {
          InterfaceStatus::Up
      } else {
          InterfaceStatus::Down
      }
  }

  fn infer_interface_type(name: &str) -> InterfaceType {
      if name.starts_with("utun") {
          InterfaceType::Vpn
      } else if name == "lo0" {
          InterfaceType::Loopback
      } else if name.starts_with("bridge") {
          InterfaceType::Bridge
      } else if name.starts_with("awdl") {
          InterfaceType::AirDrop
      } else if name.starts_with("en") {
          InterfaceType::WifiOrEthernet
      } else {
          InterfaceType::Unknown
      }
  }

  fn parse_mtu(line: &str) -> Option<u32> {
      let parts: Vec<&str> = line.split_whitespace().collect();

      parts
          .windows(2)
          .find(|window| window[0] == "mtu")
          .and_then(|window| window[1].parse::<u32>().ok())
  }
  ```

- [ ] **Step 4: 테스트 실행하여 통과 확인**
  
  Run: `cargo test --test parser_interface`
  Expected: PASS

- [ ] **Step 5: 커밋**
  
  ```bash
  git add src/collector/interface.rs
  git commit -m "feat: implement netmask parsing and heuristic network classification"
  ```

---

### Task 3: App State 고도화 및 탐색 로직 리팩토링

**Files:**
- Modify: `src/app.rs`
- Modify: `tests/app_state.rs`

- [ ] **Step 1: 실패하는 테스트 작성**
  
  `tests/app_state.rs`에 뷰 모드 전환 및 서브넷 그룹화 확인용 테스트를 추가합니다:
  ```rust
  #[test]
  fn test_app_network_view_grouping() {
      let mut app = App::default();
      // en0 (LAN), lo0 (LOOPBACK)
      let mut en0 = NetworkInterface {
          name: "en0".to_string(),
          network_kind: lazyifconfig::model::NetworkKind::Lan,
          interface_type: lazyifconfig::model::InterfaceType::WifiOrEthernet,
          status: lazyifconfig::model::InterfaceStatus::Up,
          ipv4: vec![lazyifconfig::model::InterfaceAddress {
              value: "192.168.0.15".to_string(),
              prefix_len: Some(24),
          }],
          ipv6: vec![],
          mac_address: None,
          mtu: None,
          stats: None,
      };
      let mut lo0 = NetworkInterface {
          name: "lo0".to_string(),
          network_kind: lazyifconfig::model::NetworkKind::Loopback,
          interface_type: lazyifconfig::model::InterfaceType::Loopback,
          status: lazyifconfig::model::InterfaceStatus::Up,
          ipv4: vec![lazyifconfig::model::InterfaceAddress {
              value: "127.0.0.1".to_string(),
              prefix_len: Some(8),
          }],
          ipv6: vec![],
          mac_address: None,
          mtu: None,
          stats: None,
      };

      app.replace_snapshot(lazyifconfig::model::NetworkSnapshot {
          interfaces: vec![en0, lo0],
          captured_at_secs: 100,
      });

      // 기본 뷰 모드는 Interface
      assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Interface);

      // 네트워크 뷰로 전환
      app.set_view_mode(lazyifconfig::app::ViewMode::Network);
      assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Network);

      // navigation_items 검증: SubnetHeader(127.0.0.0/8) -> lo0 -> SubnetHeader(192.168.0.0/24) -> en0
      // (정렬 순서: IPv4 중 127.0.0.0/8이 192.168.0.0/24 보다 작으므로 먼저 정렬됨)
      assert!(matches!(app.navigation_items[0], lazyifconfig::app::NavigationItem::SubnetHeader(_)));
      assert!(matches!(app.navigation_items[1], lazyifconfig::app::NavigationItem::Interface { .. }));
  }
  ```

- [ ] **Step 2: 테스트를 실행하여 실패하는지 확인**
  
  Run: `cargo test --test app_state test_app_network_view_grouping`
  Expected: 컴파일 에러.

- [ ] **Step 3: App 구현 업데이트**
  
  [src/app.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/app.rs) 전체 내용을 리팩토링합니다:
  ```rust
  use std::collections::{BTreeMap, HashMap};
  use std::net::{Ipv4Addr, Ipv6Addr};

  use crate::model::{NetworkEvent, NetworkInterface, NetworkSnapshot, Subnet, InterfaceStatus};

  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum ViewMode {
      Interface,
      Network,
  }

  #[derive(Clone, Debug, PartialEq, Eq)]
  pub enum NavigationItem {
      Interface {
          name: String,
          associated_ip: Option<String>,
      },
      SubnetHeader(Subnet),
  }

  #[derive(Clone, Debug)]
  pub struct App {
      pub current_snapshot: Option<NetworkSnapshot>,
      pub previous_snapshot: Option<NetworkSnapshot>,
      pub selected_index: usize,
      pub recent_events: Vec<NetworkEvent>,
      pub show_all: bool,
      pub view_mode: ViewMode,
      pub navigation_items: Vec<NavigationItem>,
  }

  impl Default for App {
      fn default() -> Self {
          Self {
              current_snapshot: None,
              previous_snapshot: None,
              selected_index: 0,
              recent_events: Vec::new(),
              show_all: false,
              view_mode: ViewMode::Interface,
              navigation_items: Vec::new(),
          }
      }
  }

  impl App {
      pub fn replace_snapshot(&mut self, mut snapshot: NetworkSnapshot) {
          if !self.show_all {
              snapshot.interfaces.retain(|interface| interface.status == crate::model::InterfaceStatus::Up);
          }

          let selected_name = self.selected_interface_name().map(str::to_owned);

          if let Some(previous) = self.current_snapshot.replace(snapshot) {
              self.previous_snapshot = Some(previous);
          }

          self.push_generated_events();
          self.update_navigation_items();
          self.restore_selection(selected_name.as_deref());
      }

      pub fn selected_interface_name(&self) -> Option<&str> {
          match self.navigation_items.get(self.selected_index)? {
              NavigationItem::Interface { name, .. } => Some(name.as_str()),
              NavigationItem::SubnetHeader(_) => None,
          }
      }

      pub fn set_view_mode(&mut self, mode: ViewMode) {
          if self.view_mode == mode {
              return;
          }
          let selected_name = self.selected_interface_name().map(str::to_owned);
          self.view_mode = mode;
          self.update_navigation_items();
          self.restore_selection(selected_name.as_deref());
      }

      pub fn selected_rates(&self) -> Option<(u64, u64)> {
          let current = self.current_snapshot.as_ref()?;
          let previous = self.previous_snapshot.as_ref()?;
          let elapsed = current.captured_at_secs.checked_sub(previous.captured_at_secs)?;

          if elapsed == 0 {
              return None;
          }

          let selected_name = self.selected_interface_name()?;
          let current_interface = current.interfaces.iter().find(|i| i.name == selected_name)?;
          let previous_interface = previous.interfaces.iter().find(|i| i.name == selected_name)?;
          let current_stats = current_interface.stats.as_ref()?;
          let previous_stats = previous_interface.stats.as_ref()?;

          Some((
              current_stats.rx_bytes.saturating_sub(previous_stats.rx_bytes) / elapsed,
              current_stats.tx_bytes.saturating_sub(previous_stats.tx_bytes) / elapsed,
          ))
      }

      pub fn update_navigation_items(&mut self) {
          let Some(snapshot) = &self.current_snapshot else {
              self.navigation_items = Vec::new();
              return;
          };

          match self.view_mode {
              ViewMode::Interface => {
                  self.navigation_items = snapshot
                      .interfaces
                      .iter()
                      .map(|i| NavigationItem::Interface {
                          name: i.name.clone(),
                          associated_ip: i.ipv4.first().map(|a| a.value.clone()),
                      })
                      .collect();
              }
              ViewMode::Network => {
                  let mut groups: BTreeMap<Subnet, Vec<(String, Option<String>)>> = BTreeMap::new();

                  for interface in &snapshot.interfaces {
                      let mut assigned = false;

                      // Group IPv4
                      for addr in &interface.ipv4 {
                          if let Some(prefix) = addr.prefix_len {
                              if let Ok(ip) = addr.value.parse::<Ipv4Addr>() {
                                  let net_ip = calculate_ipv4_subnet(&ip, prefix);
                                  let subnet = Subnet::Ipv4 { network: net_ip, prefix_len: prefix };
                                  groups.entry(subnet)
                                      .or_default()
                                      .push((interface.name.clone(), Some(addr.value.clone())));
                                  assigned = true;
                              }
                          }
                      }

                      // Group IPv6
                      for addr in &interface.ipv6 {
                          if let Some(prefix) = addr.prefix_len {
                              if let Ok(ip) = addr.value.parse::<Ipv6Addr>() {
                                  let net_ip = calculate_ipv6_subnet(&ip, prefix);
                                  let subnet = Subnet::Ipv6 { network: net_ip, prefix_len: prefix };
                                  groups.entry(subnet)
                                      .or_default()
                                      .push((interface.name.clone(), Some(addr.value.clone())));
                                  assigned = true;
                              }
                          }
                      }

                      if !assigned {
                          groups.entry(Subnet::Unassigned)
                              .or_default()
                              .push((interface.name.clone(), None));
                      }
                  }

                  let mut items = Vec::new();
                  for (subnet, mut members) in groups {
                      // 중복 제거 및 이름 기준 정렬
                      members.sort_by(|a, b| a.0.cmp(&b.0));
                      members.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);

                      items.push(NavigationItem::SubnetHeader(subnet));
                      for (name, ip) in members {
                          items.push(NavigationItem::Interface {
                              name,
                              associated_ip: ip,
                          });
                      }
                  }
                  self.navigation_items = items;
              }
          }
      }

      fn push_generated_events(&mut self) {
          let Some(current) = self.current_snapshot.as_ref() else {
              return;
          };

          let mut new_events = Vec::new();

          if let Some(previous) = self.previous_snapshot.as_ref() {
              let previous_by_name = interfaces_by_name(&previous.interfaces);
              let current_by_name = interfaces_by_name(&current.interfaces);

              for interface in &current.interfaces {
                  match previous_by_name.get(interface.name.as_str()) {
                      None => new_events.push(NetworkEvent::new(
                          format!("{} appeared", interface.name),
                          current.captured_at_secs,
                      )),
                      Some(previous_interface) => {
                          if previous_interface.status != interface.status {
                              new_events.push(NetworkEvent::new(
                                  format!(
                                      "{} status changed: {} -> {}",
                                      interface.name,
                                      status_label(&previous_interface.status),
                                      status_label(&interface.status)
                                  ),
                                  current.captured_at_secs,
                              ));
                          }

                          let before = first_ipv4(previous_interface);
                          let after = first_ipv4(interface);

                          if before != after {
                              if let (Some(before), Some(after)) = (before, after) {
                                  new_events.push(NetworkEvent::new(
                                      format!("{} IPv4 changed: {} -> {}", interface.name, before, after),
                                      current.captured_at_secs,
                                  ));
                              }
                          }
                      }
                  }
              }

              for interface in &previous.interfaces {
                  if !current_by_name.contains_key(interface.name.as_str()) {
                      new_events.push(NetworkEvent::new(
                          format!("{} disappeared", interface.name),
                          current.captured_at_secs,
                      ));
                  }
              }
          }

          self.recent_events.extend(new_events);

          if self.recent_events.len() > 50 {
              let overflow = self.recent_events.len() - 50;
              self.recent_events.drain(0..overflow);
          }
      }

      fn restore_selection(&mut self, selected_name: Option<&str>) {
          let len = self.navigation_items.len();
          if len == 0 {
              self.selected_index = 0;
              return;
          }

          if let Some(name) = selected_name {
              if let Some(index) = self.navigation_items.iter().position(|item| match item {
                  NavigationItem::Interface { name: item_name, .. } => item_name == name,
                  _ => false,
              }) {
                  self.selected_index = index;
                  return;
              }
          }

          if self.selected_index >= len {
              self.selected_index = len - 1;
          }
      }

      pub fn select_next(&mut self) {
          let len = self.navigation_items.len();
          if len > 0 {
              self.selected_index = (self.selected_index + 1) % len;
          }
      }

      pub fn select_previous(&mut self) {
          let len = self.navigation_items.len();
          if len > 0 {
              if self.selected_index == 0 {
                  self.selected_index = len - 1;
              } else {
                  self.selected_index -= 1;
              }
          }
      }
  }

  fn calculate_ipv4_subnet(ip: &Ipv4Addr, prefix_len: u8) -> Ipv4Addr {
      let ip_u32 = u32::from(*ip);
      let mask = if prefix_len == 0 {
          0
      } else if prefix_len >= 32 {
          u32::MAX
      } else {
          u32::MAX << (32 - prefix_len)
      };
      Ipv4Addr::from(ip_u32 & mask)
  }

  fn calculate_ipv6_subnet(ip: &Ipv6Addr, prefix_len: u8) -> Ipv6Addr {
      let octets = ip.octets();
      let mut mask_octets = [0u8; 16];
      for i in 0..16 {
          let bit_index = (i as u8) * 8;
          if prefix_len >= bit_index + 8 {
              mask_octets[i] = 0xff;
          } else if prefix_len <= bit_index {
              mask_octets[i] = 0x00;
          } else {
              let remaining = prefix_len - bit_index;
              mask_octets[i] = 0xff_u8.checked_shl((8 - remaining) as u32).unwrap_or(0);
          }
      }
      let mut subnet_octets = [0u8; 16];
      for i in 0..16 {
          subnet_octets[i] = octets[i] & mask_octets[i];
      }
      Ipv6Addr::from(subnet_octets)
  }

  fn interfaces_by_name<'a>(
      interfaces: &'a [NetworkInterface],
  ) -> HashMap<&'a str, &'a NetworkInterface> {
      interfaces
          .iter()
          .map(|interface| (interface.name.as_str(), interface))
          .collect()
  }

  fn first_ipv4(interface: &NetworkInterface) -> Option<&str> {
      interface.ipv4.first().map(|address| address.value.as_str())
  }

  fn status_label(status: &crate::model::InterfaceStatus) -> &'static str {
      match status {
          crate::model::InterfaceStatus::Up => "up",
          crate::model::InterfaceStatus::Down => "down",
      }
  }
  ```

- [ ] **Step 4: 테스트 실행하여 컴파일 오류 및 테스트 검증**
  
  일부 테스트 파일(`tests/app_state.rs`)에서 이전 평평한 `interfaces` 기반 검증 코드가 깨질 수 있습니다. 
  `tests/app_state.rs` 파일들을 열고 `app.navigation_items` 혹은 `app.replace_snapshot` 호출을 사용하는 테스트들을 컴파일 가능한 상태로 맞춰줍니다. (자세한 것은 Task 3 실행 시점에 조정)
  Run: `cargo test`
  Expected: PASS

- [ ] **Step 5: 커밋**
  
  ```bash
  git add src/app.rs
  git commit -m "feat: refactor App state to use ViewMode and NavigationItem"
  ```

---

### Task 4: UI 그리기 고도화 (서브넷 들여쓰기 및 디테일/분류 표기)

**Files:**
- Modify: `src/ui.rs`

- [ ] **Step 1: 실패하는 테스트 작성**
  
  `src/ui.rs` 테스트 모듈에서 `Network` 모드로 설정한 `App`에 대해 TUI `draw` 동작이 에러 없이 구동되는지 검증합니다.
  (기존 `test_ui_draw_no_panic`에 뷰 모드 검증 추가)
  ```rust
  #[test]
  fn test_ui_draw_network_view_no_panic() {
      let mut app = App::default();
      app.view_mode = ViewMode::Network;
      app.navigation_items = vec![
          NavigationItem::SubnetHeader(crate::model::Subnet::Unassigned),
          NavigationItem::Interface {
              name: "en0".to_string(),
              associated_ip: Some("192.168.0.15".to_string()),
          }
      ];
      let backend = TestBackend::new(80, 24);
      let mut terminal = Terminal::new(backend).unwrap();
      terminal.draw(|f| draw(f, &app)).unwrap();
  }
  ```

- [ ] **Step 2: 테스트를 실행하여 실패 확인**
  
  Run: `cargo test --lib ui::tests::test_ui_draw_network_view_no_panic`
  Expected: 컴파일 실패 (UI draw가 NavigationItem 구조 변경에 대응하지 못해 발생)

- [ ] **Step 3: UI 구현 업데이트**
  
  [src/ui.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/ui.rs)의 `draw` 함수를 아래와 같이 대대적으로 업데이트합니다:
  ```rust
  use ratatui::{
      layout::{Constraint, Direction, Layout},
      style::{Color, Modifier, Style},
      widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
      Frame,
  };
  use crate::app::{App, NavigationItem, ViewMode};
  use crate::model::{InterfaceStatus, Subnet, NetworkKind};

  pub fn render_title() -> &'static str {
      "lazyifconfig"
  }

  pub fn draw(frame: &mut Frame, app: &App) {
      let chunks = Layout::default()
          .direction(Direction::Vertical)
          .constraints([
              Constraint::Min(3),
              Constraint::Length(5),
              Constraint::Length(1),
          ])
          .split(frame.size());

      let top_chunks = Layout::default()
          .direction(Direction::Horizontal)
          .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
          .split(chunks[0]);

      // 1. Left Pane: Interfaces or Subnets list
      let title = match app.view_mode {
          ViewMode::Interface => " Interfaces ",
          ViewMode::Network => " Networks (Subnet View) ",
      };
      let list_block = Block::default().borders(Borders::ALL).title(title);
      
      let mut list_items = Vec::new();
      for (idx, item) in app.navigation_items.iter().enumerate() {
          let style = if idx == app.selected_index {
              Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
          } else {
              Style::default()
          };

          match item {
              NavigationItem::SubnetHeader(subnet) => {
                  let text = match subnet {
                      Subnet::Ipv4 { network, prefix_len } => format!("▼ {}/{}", network, prefix_len),
                      Subnet::Ipv6 { network, prefix_len } => format!("▼ {}/{}", network, prefix_len),
                      Subnet::Unassigned => "▼ Unassigned / No IP".to_string(),
                  };
                  let header_style = if idx == app.selected_index {
                      style
                  } else {
                      Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                  };
                  list_items.push(ListItem::new(text).style(header_style));
              }
              NavigationItem::Interface { name, associated_ip } => {
                  let mut status_indicator = "○";
                  let mut is_up = false;
                  let mut kind = NetworkKind::Unknown;

                  if let Some(snapshot) = &app.current_snapshot {
                      if let Some(interface) = snapshot.interfaces.iter().find(|i| i.name == *name) {
                          is_up = interface.status == InterfaceStatus::Up;
                          status_indicator = if is_up { "●" } else { "○" };
                          kind = interface.network_kind;
                      }
                  }

                  let mut display_text = if app.view_mode == ViewMode::Network {
                      format!("  {} {} ({})", status_indicator, name, associated_ip.as_deref().unwrap_or("no IP"))
                  } else {
                      format!("{} {} ({})", status_indicator, name, associated_ip.as_deref().unwrap_or("no IP"))
                  };

                  // 우측 정렬을 흉내 낸 분류 추가 (여백 맞추기)
                  let padding = 35_usize.saturating_sub(display_text.chars().count());
                  display_text.push_str(&" ".repeat(padding));
                  display_text.push_str(kind.as_str());

                  let mut final_style = style;
                  if !is_up {
                      if idx == app.selected_index {
                          final_style = final_style.add_modifier(Modifier::DIM);
                      } else {
                          final_style = final_style.fg(Color::DarkGray);
                      }
                  }
                  list_items.push(ListItem::new(display_text).style(final_style));
              }
          }
      }
      let list_widget = List::new(list_items).block(list_block);
      frame.render_widget(list_widget, top_chunks[0]);

      // 2. Right Pane: Details Panel
      let details_block = Block::default()
          .borders(Borders::ALL)
          .title(" Details ");
      
      let mut details_text = String::new();
      
      if let Some(selected_item) = app.navigation_items.get(app.selected_index) {
          match selected_item {
              NavigationItem::SubnetHeader(subnet) => {
                  details_text.push_str("=== Subnet Information ===\n\n");
                  match subnet {
                      Subnet::Ipv4 { network, prefix_len } => {
                          details_text.push_str(&format!("Protocol:       IPv4\n"));
                          details_text.push_str(&format!("Network Addr:   {}\n", network));
                          details_text.push_str(&format!("Prefix Length:  {}\n", prefix_len));
                          details_text.push_str(&format!("Subnet Mask:    {}\n", prefix_len_to_ipv4_mask(*prefix_len)));
                      }
                      Subnet::Ipv6 { network, prefix_len } => {
                          details_text.push_str(&format!("Protocol:       IPv6\n"));
                          details_text.push_str(&format!("Network Addr:   {}\n", network));
                          details_text.push_str(&format!("Prefix Length:  {}\n", prefix_len));
                      }
                      Subnet::Unassigned => {
                          details_text.push_str("Protocol:       N/A\n");
                          details_text.push_str("Description:    Interfaces without an IP Address assigned.\n");
                      }
                  }
                  
                  // 해당 서브넷에 속한 인터페이스 나열
                  details_text.push_str("\nMember Interfaces:\n");
                  if let Some(snapshot) = &app.current_snapshot {
                      for interface in &snapshot.interfaces {
                          let mut matches_subnet = false;
                          let mut ip_val = "no IP".to_string();

                          match subnet {
                              Subnet::Ipv4 { network, prefix_len } => {
                                  for addr in &interface.ipv4 {
                                      if let Some(p) = addr.prefix_len {
                                          if p == *prefix_len {
                                              if let Ok(ip) = addr.value.parse::<std::net::Ipv4Addr>() {
                                                  let net_ip = calculate_ipv4_subnet_u32(u32::from(ip), p);
                                                  if net_ip == *network {
                                                      matches_subnet = true;
                                                      ip_val = addr.value.clone();
                                                      break;
                                                  }
                                              }
                                          }
                                      }
                                  }
                              }
                              Subnet::Ipv6 { network, prefix_len } => {
                                  for addr in &interface.ipv6 {
                                      if let Some(p) = addr.prefix_len {
                                          if p == *prefix_len {
                                              if let Ok(ip) = addr.value.parse::<std::net::Ipv6Addr>() {
                                                  let net_ip = calculate_ipv6_subnet_arr(&ip, p);
                                                  if net_ip == *network {
                                                      matches_subnet = true;
                                                      ip_val = addr.value.clone();
                                                      break;
                                                  }
                                              }
                                          }
                                      }
                                  }
                              }
                              Subnet::Unassigned => {
                                  let has_ipv4 = interface.ipv4.iter().any(|a| a.prefix_len.is_some());
                                  let has_ipv6 = interface.ipv6.iter().any(|a| a.prefix_len.is_some());
                                  if !has_ipv4 && !has_ipv6 {
                                      matches_subnet = true;
                                  }
                              }
                          }

                          if matches_subnet {
                              details_text.push_str(&format!("  - {} ({})\n", interface.name, ip_val));
                          }
                      }
                  }
              }
              NavigationItem::Interface { name, .. } => {
                  if let Some(snapshot) = &app.current_snapshot {
                      if let Some(interface) = snapshot.interfaces.iter().find(|i| i.name == *name) {
                          details_text.push_str(&format!("Name:           {}\n", interface.name));
                          details_text.push_str(&format!("Classification: {}\n", interface.network_kind.as_str()));
                          details_text.push_str(&format!("Status:         {}\n", match interface.status {
                              InterfaceStatus::Up => "Active / Up",
                              InterfaceStatus::Down => "Inactive / Down",
                          }));
                          details_text.push_str(&format!("MAC Address:    {}\n", interface.mac_address.as_deref().unwrap_or("N/A")));
                          details_text.push_str(&format!("MTU:            {}\n", interface.mtu.map(|m| m.to_string()).unwrap_or_else(|| "N/A".to_string())));
                          
                          details_text.push_str("\nIPv4 Addresses:\n");
                          for addr in &interface.ipv4 {
                              details_text.push_str(&format!("  - {} / {}\n", addr.value, addr.prefix_len.map(|p| p.to_string()).unwrap_or_else(|| "?".to_string())));
                          }
                          details_text.push_str("IPv6 Addresses:\n");
                          for addr in &interface.ipv6 {
                              details_text.push_str(&format!("  - {} / {}\n", addr.value, addr.prefix_len.map(|p| p.to_string()).unwrap_or_else(|| "?".to_string())));
                          }

                          details_text.push_str("\nTraffic Statistics:\n");
                          if let Some(stats) = &interface.stats {
                              details_text.push_str(&format!("  RX Packets: {}\n", stats.rx_packets));
                              details_text.push_str(&format!("  TX Packets: {}\n", stats.tx_packets));
                              details_text.push_str(&format!("  RX Bytes:   {}\n", stats.rx_bytes));
                              details_text.push_str(&format!("  TX Bytes:   {}\n", stats.tx_bytes));
                              if let Some((rx_rate, tx_rate)) = app.selected_rates() {
                                  details_text.push_str(&format!("  RX Rate:    {} B/s\n", rx_rate));
                                  details_text.push_str(&format!("  TX Rate:    {} B/s\n", tx_rate));
                              } else {
                                  details_text.push_str("  RX Rate:    0 B/s (calculating...)\n");
                                  details_text.push_str("  TX Rate:    0 B/s (calculating...)\n");
                              }
                          } else {
                              details_text.push_str("  No stats available\n");
                          }
                      }
                  }
              }
          }
      } else {
          details_text.push_str("No data collected yet. Press 'r' to refresh.\n");
      }

      let details_p = Paragraph::new(details_text).block(details_block).wrap(Wrap { trim: true });
      frame.render_widget(details_p, top_chunks[1]);

      // 3. Event Panel
      let event_block = Block::default()
          .borders(Borders::ALL)
          .title(" Recent Events ");
      let mut event_items = Vec::new();
      for event in app.recent_events.iter().rev().take(10) {
          event_items.push(ListItem::new(format!("[{}] {}", event.captured_at_secs, event.message)));
      }
      let event_list = List::new(event_items).block(event_block);
      frame.render_widget(event_list, chunks[1]);

      // 4. Status Bar
      let status_text = format!(
          " q: Quit | r: Refresh | a: Toggle -a ({}) | i: Interface View | n: Network View | j/k: Nav ",
          if app.show_all { "ON" } else { "OFF" }
      );
      let status_p = Paragraph::new(status_text)
          .style(Style::default().bg(Color::Blue).fg(Color::White));
      frame.render_widget(status_p, chunks[2]);
  }

  fn prefix_len_to_ipv4_mask(prefix_len: u8) -> String {
      let mask = if prefix_len == 0 {
          0
      } else if prefix_len >= 32 {
          u32::MAX
      } else {
          u32::MAX << (32 - prefix_len)
      };
      let octets = std::net::Ipv4Addr::from(mask).octets();
      format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3])
  }

  fn calculate_ipv4_subnet_u32(ip_val: u32, prefix_len: u8) -> std::net::Ipv4Addr {
      let mask = if prefix_len == 0 {
          0
      } else if prefix_len >= 32 {
          u32::MAX
      } else {
          u32::MAX << (32 - prefix_len)
      };
      std::net::Ipv4Addr::from(ip_val & mask)
  }

  fn calculate_ipv6_subnet_arr(ip: &std::net::Ipv6Addr, prefix_len: u8) -> std::net::Ipv6Addr {
      let octets = ip.octets();
      let mut mask_octets = [0u8; 16];
      for i in 0..16 {
          let bit_index = (i as u8) * 8;
          if prefix_len >= bit_index + 8 {
              mask_octets[i] = 0xff;
          } else if prefix_len <= bit_index {
              mask_octets[i] = 0x00;
          } else {
              let remaining = prefix_len - bit_index;
              mask_octets[i] = 0xff_u8.checked_shl((8 - remaining) as u32).unwrap_or(0);
          }
      }
      let mut subnet_octets = [0u8; 16];
      for i in 0..16 {
          subnet_octets[i] = octets[i] & mask_octets[i];
      }
      std::net::Ipv6Addr::from(subnet_octets)
  }

  #[cfg(test)]
  mod tests {
      use super::*;
      use ratatui::{backend::TestBackend, Terminal};
      use crate::app::App;

      #[test]
      fn test_ui_draw_no_panic() {
          let app = App::default();
          let backend = TestBackend::new(80, 24);
          let mut terminal = Terminal::new(backend).unwrap();
          terminal.draw(|f| draw(f, &app)).unwrap();
          let buffer = terminal.backend().buffer();
          let mut has_borders = false;
          for cell in buffer.content() {
              if cell.symbol() == "│" || cell.symbol() == "─" {
                  has_borders = true;
                  break;
              }
          }
          assert!(has_borders);
      }

      #[test]
      fn test_ui_draw_network_view_no_panic() {
          let mut app = App::default();
          app.view_mode = ViewMode::Network;
          app.navigation_items = vec![
              NavigationItem::SubnetHeader(crate::model::Subnet::Unassigned),
              NavigationItem::Interface {
                  name: "en0".to_string(),
                  associated_ip: Some("192.168.0.15".to_string()),
              }
          ];
          let backend = TestBackend::new(80, 24);
          let mut terminal = Terminal::new(backend).unwrap();
          terminal.draw(|f| draw(f, &app)).unwrap();
      }
  }
  ```

- [ ] **Step 4: 테스트 실행하여 통과 확인**
  
  Run: `cargo test`
  Expected: PASS

- [ ] **Step 5: 커밋**
  
  ```bash
  git add src/ui.rs
  git commit -m "feat: design Network View indentation layout and subnet details renderer"
  ```

---

### Task 5: 키 바인딩 추가 및 통합 검증

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: 실패하는 테스트 작성**
  
  (main.rs의 `tick_update` 테스트 등 통합 동작이 에러 없이 도는지 확인)
  Run: `cargo test`
  Expected: 컴파일 또는 테스트 PASS (아직 변경된 키 핸들링은 단위 테스트 수준에서 검증하지 않아도 컴파일만 완료되면 정상입니다)

- [ ] **Step 2: main.rs 키 이벤트 처리 구현**
  
  [src/main.rs](file:///Users/hunchulchoi/projects/workspace/myside/tui/lazyifconfig/src/main.rs) 키 이벤트 루프에 `i`와 `n` 입력을 감지해 `App::set_view_mode`를 호출하는 핸들러를 삽입합니다:
  ```rust
  // src/main.rs 473-488 라인 근처 KeyCode 매칭 영역 수정

  match key.code {
      KeyCode::Char('q') => break,
      KeyCode::Char('r') => {
          let _ = tick_update(&mut app);
          last_tick = std::time::Instant::now();
      }
      KeyCode::Char('a') => {
          app.show_all = !app.show_all;
          let _ = tick_update(&mut app);
      }
      KeyCode::Char('i') => {
          app.set_view_mode(lazyifconfig::app::ViewMode::Interface);
      }
      KeyCode::Char('n') => {
          app.set_view_mode(lazyifconfig::app::ViewMode::Network);
      }
      KeyCode::Char('j') | KeyCode::Down => {
          app.select_next();
      }
      KeyCode::Char('k') | KeyCode::Up => {
          app.select_previous();
      }
      _ => {}
  }
  ```

- [ ] **Step 3: 테스트 및 빌드 정상 여부 검증**
  
  Run: `cargo test` 및 `cargo build`
  Expected: 컴파일 에러 없이 빌드 완수 및 모든 테스트 PASS

- [ ] **Step 4: 커밋**
  
  ```bash
  git add src/main.rs
  git commit -m "feat: bind 'i' and 'n' keys for dynamic view mode toggling"
  ```

---

## Verification Plan

### Automated Tests
- 전체 테스트 스위트를 구동하여 기존 테스트 및 신규 작성된 서브넷 연산/정렬/UI 무패닉 테스트가 모두 성공하는지 점검합니다:
  `cargo test`

### Manual Verification
1. `cargo run`을 실행하여 실행시킵니다.
2. 상태 표시줄 우측에 `LAN`, `LOOPBACK` 등의 역할 분류명이 실시간 출력되는지 확인합니다.
3. `n` 키를 눌러 **Network View**로 전환합니다.
4. 서브넷 헤더가 하늘색(`Cyan`)과 들여쓰기 구조(`▼ 192.168.0.0/24` 등)로 예쁘게 정렬되는지 확인합니다.
5. 방향키로 서브넷 헤더를 선택하고 우측 패널에 서브넷 상세 정보가 잘 노출되는지 확인합니다.
6. 개별 인터페이스를 선택하여 기존의 상세 트래픽 정보가 정상적으로 노출되는지 확인합니다.
7. `i` 키를 눌러 기존 **Interface View**로 다시 매끄럽게 복귀 및 커서가 유지되는지 확인합니다.
8. `q`를 눌러 안전하게 터미널 복구 후 복귀합니다.
