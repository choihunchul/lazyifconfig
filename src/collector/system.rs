use std::process::Command;

use crate::model::ProcessMetrics;

pub fn collect_process_metrics() -> ProcessMetrics {
    let pid = std::process::id().to_string();
    command_stdout("ps", &["-o", "%cpu=", "-o", "rss=", "-p", &pid])
        .ok()
        .and_then(|output| parse_ps_process_metrics(&output))
        .unwrap_or_default()
}

pub fn parse_ps_process_metrics(input: &str) -> Option<ProcessMetrics> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let cpu = parts[parts.len() - 2].parse::<f64>().ok()?;
    let rss_kib = parts[parts.len() - 1].parse::<u64>().ok()?;

    Some(ProcessMetrics {
        cpu_usage_tenths: Some((cpu * 10.0).round().clamp(0.0, u16::MAX as f64) as u16),
        memory_rss_bytes: Some(rss_kib * 1024),
    })
}

fn command_stdout(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        String::from_utf8(output.stdout).map_err(|err| err.to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}
