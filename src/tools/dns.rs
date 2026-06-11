use super::{run_command, ToolCommandSpec, ToolInput, ToolResult, ToolResultSection};

pub fn command_candidates(target: &str) -> Vec<ToolCommandSpec> {
    vec![
        ToolCommandSpec {
            display: format!("dig {target}"),
            program: "dig".to_string(),
            args: vec![target.to_string()],
        },
        ToolCommandSpec {
            display: format!("host {target}"),
            program: "host".to_string(),
            args: vec![target.to_string()],
        },
        ToolCommandSpec {
            display: format!("nslookup {target}"),
            program: "nslookup".to_string(),
            args: vec![target.to_string()],
        },
    ]
}

pub async fn run(input: ToolInput) -> Result<ToolResult, String> {
    let target = input.get("target").unwrap_or("").trim();
    if target.is_empty() {
        return Err("Target is required.".to_string());
    }

    let mut failures = Vec::new();
    for spec in command_candidates(target) {
        match run_command(&spec).await {
            Ok((stdout, stderr, code)) => {
                let raw_output = format!("$ {}\n{}{}", spec.display, stdout, stderr);
                let mut lines = Vec::new();
                if code == Some(0) {
                    lines.push("Command completed successfully.".to_string());
                } else {
                    lines.push(format!("Command exited with status {:?}.", code));
                }
                for line in stdout.lines().take(8) {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        lines.push(trimmed.to_string());
                    }
                }

                return Ok(ToolResult {
                    title: "DNS Lookup".to_string(),
                    sections: vec![ToolResultSection {
                        label: "Result".to_string(),
                        lines,
                    }],
                    raw_output,
                });
            }
            Err(err) => failures.push(format!("{} ({})", spec.program, err)),
        }
    }

    Err(format!(
        "No DNS command succeeded. Tried: {}",
        failures.join(", ")
    ))
}
