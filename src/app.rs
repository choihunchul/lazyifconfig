use std::collections::{BTreeMap, HashMap};
use std::net::{Ipv4Addr, Ipv6Addr};

use crate::model::{NetworkEvent, NetworkInterface, NetworkSnapshot, Subnet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Interface,
    Network,
    Connections,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavigationItem {
    Interface {
        name: String,
        associated_ip: Option<String>,
    },
    SubnetHeader(Subnet),
    Connection {
        proto: String,
        local: String,
        foreign: String,
        state: Option<String>,
        index: usize,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InterfaceHistory {
    pub rx_rates: Vec<u64>,
    pub tx_rates: Vec<u64>,
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
    pub traffic_history: HashMap<String, InterfaceHistory>,
    pub whois_cache: std::sync::Arc<std::sync::Mutex<HashMap<String, String>>>,
    pub details_scroll: u16,
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
            traffic_history: HashMap::new(),
            whois_cache: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            details_scroll: 0,
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

        // Update traffic history
        if let (Some(previous), Some(current)) = (&self.previous_snapshot, &self.current_snapshot) {
            let elapsed = current.captured_at_secs.saturating_sub(previous.captured_at_secs);
            if elapsed > 0 {
                let previous_by_name = interfaces_by_name(&previous.interfaces);
                for interface in &current.interfaces {
                    if let Some(prev_if) = previous_by_name.get(interface.name.as_str()) {
                        if let (Some(curr_stats), Some(prev_stats)) = (&interface.stats, &prev_if.stats) {
                            let rx_rate = curr_stats.rx_bytes.saturating_sub(prev_stats.rx_bytes) / elapsed;
                            let tx_rate = curr_stats.tx_bytes.saturating_sub(prev_stats.tx_bytes) / elapsed;

                            let history = self.traffic_history.entry(interface.name.clone()).or_default();
                            history.rx_rates.push(rx_rate);
                            history.tx_rates.push(tx_rate);

                            if history.rx_rates.len() > 40 {
                                history.rx_rates.remove(0);
                            }
                            if history.tx_rates.len() > 40 {
                                history.tx_rates.remove(0);
                            }
                        }
                    }
                }
            }
        }

        // Clean up history for removed interfaces
        if let Some(current) = &self.current_snapshot {
            self.traffic_history.retain(|name, _| {
                current.interfaces.iter().any(|i| i.name == *name)
            });
        }

        self.push_generated_events();
        self.update_navigation_items();
        self.restore_selection(selected_name.as_deref());
    }

    pub fn selected_interface_name(&self) -> Option<&str> {
        match self.navigation_items.get(self.selected_index)? {
            NavigationItem::Interface { name, .. } => Some(name.as_str()),
            NavigationItem::SubnetHeader(_) => None,
            NavigationItem::Connection { .. } => None,
        }
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) {
        if self.view_mode == mode {
            return;
        }
        let selected_name = self.selected_interface_name().map(str::to_owned);
        self.view_mode = mode;
        self.details_scroll = 0;
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
                    // Sort members by interface name
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
            ViewMode::Connections => {
                self.navigation_items = snapshot
                    .connections
                    .iter()
                    .enumerate()
                    .map(|(idx, c)| NavigationItem::Connection {
                        proto: c.proto.clone(),
                        local: format!("{}:{}", c.local_ip, c.local_port),
                        foreign: format!("{}:{}", c.foreign_ip, c.foreign_port),
                        state: c.state.clone(),
                        index: idx,
                    })
                    .collect();
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
            self.details_scroll = 0;
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
            self.details_scroll = 0;
        }
    }

    pub fn scroll_details_down(&mut self) {
        self.details_scroll = self.details_scroll.saturating_add(1);
    }

    pub fn scroll_details_up(&mut self) {
        self.details_scroll = self.details_scroll.saturating_sub(1);
    }

    pub fn get_whois_result(&self, ip: &str) -> Option<String> {
        let lock = self.whois_cache.lock().ok()?;
        lock.get(ip).cloned()
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
