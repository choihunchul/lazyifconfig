use std::collections::BTreeMap;
use std::time::Duration;

pub mod dns;
pub mod ping;
pub mod port_check;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolId {
    DnsLookup,
    WhoisLookup,
    IpInformation,
    PortCheck,
    TlsInspector,
    Ping,
    Traceroute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolAvailability {
    Runnable,
    Planned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolField {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolDefinition {
    pub id: ToolId,
    pub name: &'static str,
    pub description: &'static str,
    pub availability: ToolAvailability,
    pub fields: &'static [ToolField],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolInput {
    pub values: BTreeMap<String, String>,
}

impl ToolInput {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResultSection {
    pub label: String,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResult {
    pub title: String,
    pub sections: Vec<ToolResultSection>,
    pub raw_output: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolExecutionState {
    Idle,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug)]
pub struct ToolRegistry {
    definitions: Vec<ToolDefinition>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            definitions: vec![
                ToolDefinition {
                    id: ToolId::DnsLookup,
                    name: "DNS Lookup",
                    description: "Resolve DNS records for a domain or IP.",
                    availability: ToolAvailability::Runnable,
                    fields: &[ToolField {
                        key: "target",
                        label: "Target",
                        placeholder: "example.com",
                    }],
                },
                ToolDefinition {
                    id: ToolId::WhoisLookup,
                    name: "Whois Lookup",
                    description: "Look up domain or IP ownership metadata.",
                    availability: ToolAvailability::Planned,
                    fields: &[ToolField {
                        key: "target",
                        label: "Target",
                        placeholder: "github.com",
                    }],
                },
                ToolDefinition {
                    id: ToolId::IpInformation,
                    name: "IP Information",
                    description: "Summarize ASN, organization, country, and reverse DNS.",
                    availability: ToolAvailability::Planned,
                    fields: &[ToolField {
                        key: "ip",
                        label: "IP",
                        placeholder: "8.8.8.8",
                    }],
                },
                ToolDefinition {
                    id: ToolId::PortCheck,
                    name: "Port Check",
                    description: "Check TCP connectivity to a host and port.",
                    availability: ToolAvailability::Runnable,
                    fields: &[
                        ToolField {
                            key: "host",
                            label: "Host",
                            placeholder: "github.com",
                        },
                        ToolField {
                            key: "port",
                            label: "Port",
                            placeholder: "443",
                        },
                    ],
                },
                ToolDefinition {
                    id: ToolId::TlsInspector,
                    name: "TLS Inspector",
                    description: "Inspect certificate and TLS details.",
                    availability: ToolAvailability::Planned,
                    fields: &[ToolField {
                        key: "target",
                        label: "Target",
                        placeholder: "github.com:443",
                    }],
                },
                ToolDefinition {
                    id: ToolId::Ping,
                    name: "Ping",
                    description: "Measure reachability and latency with ping.",
                    availability: ToolAvailability::Runnable,
                    fields: &[ToolField {
                        key: "target",
                        label: "Target",
                        placeholder: "8.8.8.8",
                    }],
                },
                ToolDefinition {
                    id: ToolId::Traceroute,
                    name: "Traceroute",
                    description: "Visualize the packet path to a target.",
                    availability: ToolAvailability::Planned,
                    fields: &[ToolField {
                        key: "target",
                        label: "Target",
                        placeholder: "8.8.8.8",
                    }],
                },
            ],
        }
    }
}

impl ToolRegistry {
    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    pub fn definition(&self, id: ToolId) -> Option<&ToolDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCommandSpec {
    pub display: String,
    pub program: String,
    pub args: Vec<String>,
}

pub async fn run_tool(id: ToolId, input: ToolInput) -> Result<ToolResult, String> {
    match id {
        ToolId::DnsLookup => dns::run(input).await,
        ToolId::PortCheck => port_check::run(input, Duration::from_secs(3)).await,
        ToolId::Ping => ping::run(input).await,
        _ => Err("This tool is planned and is not executable yet.".to_string()),
    }
}

pub async fn run_command(spec: &ToolCommandSpec) -> Result<(String, String, Option<i32>), String> {
    let output = tokio::process::Command::new(&spec.program)
        .args(&spec.args)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    Ok((
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code(),
    ))
}
