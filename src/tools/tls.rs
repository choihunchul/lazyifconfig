use super::{run_command, ToolCommandSpec, ToolInput, ToolResult, ToolResultSection};

pub fn command_spec(host: &str, port: u16) -> ToolCommandSpec {
    ToolCommandSpec {
        display: format!("openssl s_client -connect {host}:{port} -servername {host} -showcerts"),
        program: "openssl".to_string(),
        args: vec![
            "s_client".to_string(),
            "-connect".to_string(),
            format!("{host}:{port}"),
            "-servername".to_string(),
            host.to_string(),
            "-showcerts".to_string(),
        ],
    }
}

pub async fn run(input: ToolInput) -> Result<ToolResult, String> {
    let target = input.get("target").unwrap_or("").trim();
    if target.is_empty() {
        return Err("Target is required.".to_string());
    }

    let (host, port) = parse_target(target)?;
    let spec = command_spec(&host, port);
    let (stdout, stderr, code) = run_command(&spec).await?;
    let raw_output = format!("$ {}\n{}{}", spec.display, stdout, stderr);
    let sections = parse_tls_output(&host, port, &stdout, &stderr, code);

    Ok(ToolResult {
        title: "TLS Inspector".to_string(),
        sections,
        raw_output,
    })
}

fn parse_target(target: &str) -> Result<(String, u16), String> {
    if let Some((host, port_raw)) = target.rsplit_once(':') {
        let port = port_raw
            .parse::<u16>()
            .map_err(|_| "Port must be a number from 1 to 65535.".to_string())?;
        if host.trim().is_empty() || port == 0 {
            return Err("Target must look like host:port.".to_string());
        }
        return Ok((host.trim().to_string(), port));
    }

    Ok((target.to_string(), 443))
}

fn parse_tls_output(
    host: &str,
    port: u16,
    stdout: &str,
    stderr: &str,
    code: Option<i32>,
) -> Vec<ToolResultSection> {
    let combined = format!("{stdout}\n{stderr}");
    let protocol = find_prefixed_value(&combined, "Protocol  :")
        .or_else(|| find_prefixed_value(&combined, "Protocol:"));
    let cipher = find_prefixed_value(&combined, "Cipher    :")
        .or_else(|| find_prefixed_value(&combined, "Cipher:"));
    let subject = find_prefixed_value(&combined, "subject=");
    let issuer = find_prefixed_value(&combined, "issuer=");
    let verify = find_prefixed_value(&combined, "Verify return code:");

    let mut summary = vec![format!("Target: {host}:{port}")];
    summary.push(format!(
        "Status: {}",
        if code == Some(0) {
            "Handshake completed"
        } else {
            "Handshake returned a non-zero status"
        }
    ));
    if let Some(value) = protocol.clone() {
        summary.push(format!("Protocol: {value}"));
    }
    if let Some(value) = cipher.clone() {
        summary.push(format!("Cipher: {value}"));
    }

    let mut sections = vec![ToolResultSection {
        label: "Summary".to_string(),
        lines: summary,
    }];

    let mut certificate = Vec::new();
    if let Some(value) = subject {
        certificate.push(format!("Subject: {value}"));
    }
    if let Some(value) = issuer {
        certificate.push(format!("Issuer: {value}"));
    }
    if let Some(value) = verify.clone() {
        certificate.push(format!("Verify: {value}"));
    }
    if !certificate.is_empty() {
        sections.push(ToolResultSection {
            label: "Certificate".to_string(),
            lines: certificate,
        });
    }

    let mut diagnostics = Vec::new();
    if protocol.is_none() || cipher.is_none() {
        diagnostics.push("OpenSSL output was partial; inspect raw output for handshake details.".to_string());
    }
    if !stderr.trim().is_empty() {
        diagnostics.push(stderr.trim().to_string());
    }
    if diagnostics.is_empty() {
        diagnostics.push("TLS handshake metadata parsed successfully.".to_string());
    }
    sections.push(ToolResultSection {
        label: "Diagnostics".to_string(),
        lines: diagnostics,
    });

    sections
}

fn find_prefixed_value(text: &str, prefix: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix(prefix)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}
