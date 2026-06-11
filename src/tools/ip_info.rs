use std::net::IpAddr;

use serde_json::Value;

use super::{run_command, ToolCommandSpec, ToolInput, ToolResult, ToolResultSection};
use crate::tools::whois::{first_field, parse_whois_output};

pub fn reverse_dns_command_candidates(ip: &str) -> Vec<ToolCommandSpec> {
    reverse_dns_command_candidates_for_os(std::env::consts::OS, ip)
}

pub fn reverse_dns_command_candidates_for_os(os: &str, ip: &str) -> Vec<ToolCommandSpec> {
    if os == "windows" {
        return vec![ToolCommandSpec {
            display: format!("nslookup {ip}"),
            program: "nslookup".to_string(),
            args: vec![ip.to_string()],
        }];
    }

    vec![
        ToolCommandSpec {
            display: format!("dig -x {ip} +short"),
            program: "dig".to_string(),
            args: vec!["-x".to_string(), ip.to_string(), "+short".to_string()],
        },
        ToolCommandSpec {
            display: format!("host {ip}"),
            program: "host".to_string(),
            args: vec![ip.to_string()],
        },
        ToolCommandSpec {
            display: format!("nslookup {ip}"),
            program: "nslookup".to_string(),
            args: vec![ip.to_string()],
        },
    ]
}

pub async fn run(input: ToolInput) -> Result<ToolResult, String> {
    let ip = input.get("ip").unwrap_or("").trim();
    if ip.is_empty() {
        return Err("IP is required.".to_string());
    }
    ip.parse::<IpAddr>()
        .map_err(|_| "IP must be a valid IPv4 or IPv6 address.".to_string())?;

    let mut raw_chunks = Vec::new();
    let mut reverse_name = None;
    let mut reverse_error = None;
    let mut rdap_value = None;

    for spec in reverse_dns_command_candidates(ip) {
        match run_command(&spec).await {
            Ok((stdout, stderr, _code)) => {
                raw_chunks.push(format!("$ {}\n{}{}", spec.display, stdout, stderr));
                reverse_name =
                    parse_reverse_dns_output(&stdout).or_else(|| parse_reverse_dns_output(&stderr));
                if reverse_name.is_some() {
                    break;
                }
            }
            Err(err) => reverse_error = Some(format!("{} ({err})", spec.program)),
        }
    }

    let whois_spec = super::whois::command_spec(ip);
    let (whois_stdout, whois_stderr, whois_code) = if std::env::consts::OS == "windows" {
        match run_ip_rdap(ip).await {
            Ok((stdout, stderr, code, value, display)) => {
                raw_chunks.push(format!("$ {display}\n{}{}", stdout, stderr));
                rdap_value = Some(value);
                (stdout, stderr, code)
            }
            Err(rdap_err) => (
                "".to_string(),
                format!("RDAP lookup failed: {rdap_err}"),
                None,
            ),
        }
    } else {
        match run_command(&whois_spec).await {
            Ok(output) => output,
            Err(err) => {
                raw_chunks.push(format!("$ {}\n{}", whois_spec.display, err));
                match run_ip_rdap(ip).await {
                    Ok((stdout, stderr, code, value, display)) => {
                        raw_chunks.push(format!("$ {display}\n{}{}", stdout, stderr));
                        rdap_value = Some(value);
                        (stdout, stderr, code)
                    }
                    Err(rdap_err) => (
                        "".to_string(),
                        format!("{err}; RDAP fallback failed: {rdap_err}"),
                        None,
                    ),
                }
            }
        }
    };
    if (!whois_stdout.is_empty() || !whois_stderr.is_empty()) && rdap_value.is_none() {
        raw_chunks.push(format!(
            "$ {}\n{}{}",
            whois_spec.display, whois_stdout, whois_stderr
        ));
    }

    let combined = if whois_stdout.is_empty() {
        whois_stderr.as_str()
    } else {
        whois_stdout.as_str()
    };
    let organization = first_field(
        combined,
        &[
            "OrgName",
            "org-name",
            "org",
            "Organization",
            "descr",
            "owner",
        ],
    )
    .or_else(|| rdap_value.as_ref().and_then(rdap_org_name));
    let country = first_field(combined, &["Country", "country"]).or_else(|| {
        rdap_value
            .as_ref()
            .and_then(|value| rdap_text_field(value, "country"))
    });
    let asn = first_field(combined, &["origin", "originas", "aut-num", "ASNumber"]);

    let mut sections = vec![ToolResultSection {
        label: "Summary".to_string(),
        lines: vec![
            format!("IP: {ip}"),
            format!(
                "Reverse DNS: {}",
                reverse_name
                    .clone()
                    .unwrap_or_else(|| "Unavailable".to_string())
            ),
            format!(
                "Organization: {}",
                organization
                    .clone()
                    .unwrap_or_else(|| "Unavailable".to_string())
            ),
            format!(
                "ASN: {}",
                asn.clone().unwrap_or_else(|| "Unavailable".to_string())
            ),
            format!(
                "Country: {}",
                country.clone().unwrap_or_else(|| "Unavailable".to_string())
            ),
        ],
    }];

    let whois_sections = parse_whois_output(ip, &whois_stdout, &whois_stderr, whois_code);
    if let Some(dates) = whois_sections
        .into_iter()
        .find(|section| section.label == "Dates")
    {
        sections.push(dates);
    }

    let mut diagnostics = Vec::new();
    if reverse_name.is_none() {
        diagnostics
            .push(reverse_error.unwrap_or_else(|| "No reverse DNS answer was parsed.".to_string()));
    }
    if organization.is_none() && asn.is_none() && country.is_none() {
        diagnostics.push("Whois ownership details were limited for this IP.".to_string());
    }
    if diagnostics.is_empty() {
        diagnostics.push("Reverse DNS and registration metadata collected.".to_string());
    }
    sections.push(ToolResultSection {
        label: "Diagnostics".to_string(),
        lines: diagnostics,
    });

    Ok(ToolResult {
        title: "IP Information".to_string(),
        sections,
        raw_output: raw_chunks.join("\n"),
    })
}

async fn run_ip_rdap(ip: &str) -> Result<(String, String, Option<i32>, Value, String), String> {
    let url = format!("https://rdap.org/ip/{ip}");
    let spec = ToolCommandSpec {
        display: format!("curl -sS -L -m 10 {url}"),
        program: "curl".to_string(),
        args: vec![
            "-sS".to_string(),
            "-L".to_string(),
            "-m".to_string(),
            "10".to_string(),
            url,
        ],
    };
    let display = spec.display.clone();
    let (stdout, stderr, code) = run_command(&spec).await?;
    let value: Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("RDAP response was not valid JSON: {e}"))?;
    Ok((stdout, stderr, code, value, display))
}

fn rdap_org_name(value: &Value) -> Option<String> {
    rdap_text_field(value, "name")
        .or_else(|| rdap_text_field(value, "handle"))
        .or_else(|| {
            let start = rdap_text_field(value, "startAddress")?;
            let end = rdap_text_field(value, "endAddress")?;
            Some(format!("{start} - {end}"))
        })
}

fn rdap_text_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_reverse_dns_output(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim().trim_end_matches('.');
        if trimmed.is_empty() {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("domain name pointer ") {
            return Some(value.trim().trim_end_matches('.').to_string());
        }
        if trimmed.contains(" domain name pointer ") {
            return trimmed
                .split(" domain name pointer ")
                .nth(1)
                .map(|value| value.trim().trim_end_matches('.').to_string());
        }
        if let Some(value) = trimmed.strip_prefix("Name:") {
            let name = value.trim().trim_end_matches('.');
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
        if trimmed.contains('.') && !trimmed.contains(' ') {
            return Some(trimmed.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_windows_nslookup_reverse_name() {
        let output = "\
Server:  UnKnown
Address:  192.168.0.1

Name:    dns.google
Address:  8.8.8.8
";

        assert_eq!(
            parse_reverse_dns_output(output).as_deref(),
            Some("dns.google")
        );
    }

    #[test]
    fn parses_rdap_ip_metadata() {
        let value: Value = serde_json::json!({
            "name": "GOGL",
            "country": "US",
            "startAddress": "8.8.8.0",
            "endAddress": "8.8.8.255"
        });

        assert_eq!(rdap_org_name(&value).as_deref(), Some("GOGL"));
        assert_eq!(rdap_text_field(&value, "country").as_deref(), Some("US"));
    }
}
