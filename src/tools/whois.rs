use super::{run_command, ToolCommandSpec, ToolInput, ToolResult, ToolResultSection};

pub fn command_spec(target: &str) -> ToolCommandSpec {
    ToolCommandSpec {
        display: format!("whois {target}"),
        program: "whois".to_string(),
        args: vec![target.to_string()],
    }
}

pub async fn run(input: ToolInput) -> Result<ToolResult, String> {
    let target = input.get("target").unwrap_or("").trim();
    if target.is_empty() {
        return Err("Target is required.".to_string());
    }

    let spec = command_spec(target);
    let (stdout, stderr, code) = run_command(&spec).await?;
    let raw_output = format!("$ {}\n{}{}", spec.display, stdout, stderr);
    let parsed = parse_whois_output(target, &stdout, &stderr, code);

    Ok(ToolResult {
        title: "Whois Lookup".to_string(),
        sections: parsed,
        raw_output,
    })
}

pub(crate) fn parse_whois_output(
    target: &str,
    stdout: &str,
    stderr: &str,
    code: Option<i32>,
) -> Vec<ToolResultSection> {
    let combined = if stdout.is_empty() {
        stderr
    } else {
        stdout
    };
    let registrar = first_field(combined, &["Registrar", "registrar"]);
    let organization = first_field(
        combined,
        &[
            "OrgName",
            "org-name",
            "org",
            "Organization",
            "Registrant Organization",
            "owner",
        ],
    );
    let country = first_field(combined, &["Country", "Registrant Country", "country"]);
    let created = first_field(
        combined,
        &["Creation Date", "Created On", "created", "Registration Time"],
    );
    let updated = first_field(combined, &["Updated Date", "Last Updated On", "updated"]);
    let expiry = first_field(
        combined,
        &[
            "Registry Expiry Date",
            "Registrar Registration Expiration Date",
            "Expiry Date",
            "paid-till",
            "expires",
        ],
    );
    let name_servers = collect_fields(combined, &["Name Server", "nserver", "Name Servers"]);

    let mut summary = vec![format!("Target: {target}")];
    if code == Some(0) {
        summary.push("Status: Lookup completed".to_string());
    } else {
        summary.push(format!("Status: Command exited with status {:?}", code));
    }
    if let Some(value) = registrar.clone() {
        summary.push(format!("Registrar: {value}"));
    }
    if let Some(value) = organization.clone() {
        summary.push(format!("Organization: {value}"));
    }
    if let Some(value) = country.clone() {
        summary.push(format!("Country: {value}"));
    }

    let mut sections = vec![ToolResultSection {
        label: "Summary".to_string(),
        lines: summary,
    }];

    let mut dates = Vec::new();
    if let Some(value) = created {
        dates.push(format!("Created: {value}"));
    }
    if let Some(value) = updated {
        dates.push(format!("Updated: {value}"));
    }
    if let Some(value) = expiry {
        dates.push(format!("Expires: {value}"));
    }
    if !dates.is_empty() {
        sections.push(ToolResultSection {
            label: "Dates".to_string(),
            lines: dates,
        });
    }

    if !name_servers.is_empty() {
        sections.push(ToolResultSection {
            label: "Name Servers".to_string(),
            lines: name_servers,
        });
    }

    let mut diagnostics = Vec::new();
    if registrar.is_none() && organization.is_none() && country.is_none() {
        diagnostics.push("Parsed metadata was limited; inspect raw output for full details.".to_string());
    }
    if !stderr.trim().is_empty() {
        diagnostics.push(stderr.trim().to_string());
    }
    if diagnostics.is_empty() {
        diagnostics.push("Whois lookup returned structured ownership details.".to_string());
    }
    sections.push(ToolResultSection {
        label: "Diagnostics".to_string(),
        lines: diagnostics,
    });

    sections
}

pub(crate) fn first_field(text: &str, names: &[&str]) -> Option<String> {
    text.lines().find_map(|line| {
        names
            .iter()
            .find_map(|name| extract_named_field(line, name))
            .filter(|value| !value.is_empty())
    })
}

pub(crate) fn collect_fields(text: &str, names: &[&str]) -> Vec<String> {
    let mut values = Vec::new();
    for line in text.lines() {
        for name in names {
            if let Some(value) = extract_named_field(line, name) {
                if !value.is_empty() && !values.contains(&value) {
                    values.push(value);
                }
            }
        }
    }
    values
}

fn extract_named_field(line: &str, name: &str) -> Option<String> {
    let trimmed = line.trim();
    let lower = trimmed.to_lowercase();
    let name_lower = name.to_lowercase();

    for separator in [':', '='] {
        let pattern = format!("{name_lower}{separator}");
        if lower.starts_with(&pattern) {
            return Some(trimmed[name.len() + 1..].trim().to_string());
        }
    }

    None
}
