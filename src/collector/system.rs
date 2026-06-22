use std::process::Command;

use crate::model::{JavaDetails, ProcessDetails, ProcessMetrics};

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

pub fn collect_process_details(pid: &str) -> Option<ProcessDetails> {
    if pid == "-" || pid.is_empty() {
        return None;
    }

    let mut details = command_stdout(
        "ps",
        &[
            "-p", pid, "-o", "%cpu=", "-o", "rss=", "-o", "nlwp=", "-o", "lstart=", "-o",
            "command=",
        ],
    )
    .ok()
    .and_then(|output| parse_ps_process_details(&output))
    .unwrap_or_default();

    if details.executable.is_none() {
        details.executable = command_stdout("ps", &["-p", pid, "-o", "comm="])
            .ok()
            .map(|output| output.trim().to_string())
            .filter(|output| !output.is_empty());
    }

    if details.working_dir.is_none() {
        details.working_dir = command_stdout("lsof", &["-a", "-p", pid, "-d", "cwd", "-Fn"])
            .ok()
            .and_then(|output| parse_lsof_cwd(&output));
    }

    if details == ProcessDetails::default() {
        None
    } else {
        Some(details)
    }
}

pub fn parse_ps_process_details(input: &str) -> Option<ProcessDetails> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() < 9 {
        return None;
    }

    let cpu = parts[0].parse::<f64>().ok();
    let rss_kib = parts[1].parse::<u64>().ok();
    let threads = parts[2].parse::<u32>().ok();
    let started_raw = parts[3..8].join(" ");
    let command_line = parts[8..].join(" ");
    let executable = command_line.split_whitespace().next().map(str::to_string);

    Some(ProcessDetails {
        executable,
        working_dir: None,
        started: Some(format_started_time(&started_raw)),
        cpu_usage_tenths: cpu.map(|cpu| (cpu * 10.0).round().clamp(0.0, u16::MAX as f64) as u16),
        memory_rss_bytes: rss_kib.map(|rss| rss * 1024),
        threads,
        java: parse_java_details(&command_line),
    })
}

pub fn parse_java_details(command_line: &str) -> Option<JavaDetails> {
    if !command_line.contains("java") {
        return None;
    }

    let parts: Vec<&str> = command_line.split_whitespace().collect();
    let mut details = JavaDetails::default();
    let mut index = 0;
    while index < parts.len() {
        let part = parts[index];
        if let Some(xmx) = part.strip_prefix("-Xmx") {
            details.xmx = Some(xmx.to_string());
        } else if let Some(xms) = part.strip_prefix("-Xms") {
            details.xms = Some(xms.to_string());
        } else if part == "-jar" {
            details.jar = parts.get(index + 1).map(|jar| jar.to_string());
            index += 1;
        } else if part.ends_with(".jar") && details.jar.is_none() {
            details.jar = Some(part.to_string());
        }
        index += 1;
    }

    if details == JavaDetails::default() {
        None
    } else {
        Some(details)
    }
}

fn parse_lsof_cwd(input: &str) -> Option<String> {
    input
        .lines()
        .find_map(|line| line.strip_prefix('n').map(str::to_string))
        .filter(|path| !path.is_empty())
}

fn format_started_time(input: &str) -> String {
    chrono::NaiveDateTime::parse_from_str(input, "%a %b %e %H:%M:%S %Y")
        .map(|time| time.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| input.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_process_details_from_ps_output() {
        let input = "12.4 1757812 84 Mon Jun 22 09:12:00 2026 /usr/lib/jvm/java-21/bin/java -Xmx2G -Xms512M -jar monitor.jar";

        let details = parse_ps_process_details(input).expect("details");

        assert_eq!(
            details.executable.as_deref(),
            Some("/usr/lib/jvm/java-21/bin/java")
        );
        assert_eq!(details.started.as_deref(), Some("2026-06-22 09:12"));
        assert_eq!(details.cpu_usage_tenths, Some(124));
        assert_eq!(details.memory_rss_bytes, Some(1_757_812 * 1024));
        assert_eq!(details.threads, Some(84));
        let java = details.java.expect("java details");
        assert_eq!(java.xmx.as_deref(), Some("2G"));
        assert_eq!(java.xms.as_deref(), Some("512M"));
        assert_eq!(java.jar.as_deref(), Some("monitor.jar"));
    }

    #[test]
    fn parses_lsof_cwd_name_line() {
        assert_eq!(
            parse_lsof_cwd("p90759\nfcwd\nn/opt/tibero/monitor\n").as_deref(),
            Some("/opt/tibero/monitor")
        );
    }
}
