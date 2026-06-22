#[cfg(target_os = "windows")]
use std::sync::{Mutex, OnceLock};
use std::{collections::HashMap, process::Command};

use crate::model::{JavaDetails, ProcessDetails, ProcessMetrics};
use serde_json::Value;

#[cfg(target_os = "windows")]
#[derive(Clone, Copy, Debug, Default)]
struct WindowsMetricsSample {
    process_time_100ns: u64,
    system_time_100ns: u64,
}

#[cfg(target_os = "windows")]
static WINDOWS_METRICS_SAMPLE: OnceLock<Mutex<Option<WindowsMetricsSample>>> = OnceLock::new();

pub fn collect_process_metrics() -> ProcessMetrics {
    if cfg!(target_os = "windows") {
        return collect_windows_process_metrics().unwrap_or_default();
    }

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

pub fn windows_cpu_usage_tenths(
    previous_process_time_100ns: u64,
    current_process_time_100ns: u64,
    elapsed_system_time_100ns: u64,
    logical_cpu_count: u32,
) -> Option<u16> {
    if logical_cpu_count == 0 || elapsed_system_time_100ns == 0 {
        return None;
    }

    let process_delta = current_process_time_100ns.checked_sub(previous_process_time_100ns)? as f64;
    let available_cpu_time = elapsed_system_time_100ns as f64 * logical_cpu_count as f64;
    Some(
        ((process_delta / available_cpu_time) * 1000.0)
            .round()
            .clamp(0.0, u16::MAX as f64) as u16,
    )
}

pub fn collect_process_details(pid: &str) -> Option<ProcessDetails> {
    if pid == "-" || pid.is_empty() {
        return None;
    }

    if cfg!(target_os = "windows") {
        return collect_windows_process_details(pid);
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
        command_line: Some(command_line.clone()).filter(|command| !command.trim().is_empty()),
        service: None,
        working_dir: None,
        started: Some(format_started_time(&started_raw)),
        cpu_usage_tenths: cpu.map(|cpu| (cpu * 10.0).round().clamp(0.0, u16::MAX as f64) as u16),
        memory_rss_bytes: rss_kib.map(|rss| rss * 1024),
        threads,
        java: parse_java_details(&command_line),
    })
}

pub fn parse_windows_process_details(input: &str) -> Option<ProcessDetails> {
    let value: Value = serde_json::from_str(input).ok()?;
    let process = value.get("Process").unwrap_or(&value);
    let executable = string_field(process, "ExecutablePath")
        .or_else(|| command_executable(string_field(process, "CommandLine").as_deref()));
    let command_line = string_field(process, "CommandLine");
    let started = string_field(process, "CreationDate").map(|date| format_windows_cim_date(&date));
    let threads = u64_field(process, "ThreadCount").and_then(|threads| threads.try_into().ok());
    let memory_rss_bytes = u64_field(process, "WorkingSetSize");
    let service = value
        .get("Services")
        .map(value_items)
        .unwrap_or_default()
        .into_iter()
        .filter_map(service_label)
        .collect::<Vec<_>>()
        .join(", ");
    let service = if service.is_empty() {
        None
    } else {
        Some(service)
    };

    let details = ProcessDetails {
        executable,
        command_line: command_line.clone(),
        service,
        working_dir: None,
        started,
        cpu_usage_tenths: None,
        memory_rss_bytes,
        threads,
        java: command_line.as_deref().and_then(parse_java_details),
    };

    if details == ProcessDetails::default() {
        None
    } else {
        Some(details)
    }
}

pub fn parse_windows_process_details_by_pid(input: &str) -> HashMap<String, ProcessDetails> {
    let Ok(value) = serde_json::from_str::<Value>(input) else {
        return HashMap::new();
    };

    value_items(&value)
        .into_iter()
        .filter_map(|item| {
            let pid = string_field(item, "ProcessId")?;
            parse_windows_process_details_value(item).map(|details| (pid, details))
        })
        .collect()
}

pub fn collect_windows_process_details_by_pid(pids: &[String]) -> HashMap<String, ProcessDetails> {
    let Some(filter) = windows_process_filter(pids) else {
        return HashMap::new();
    };

    let script = format!(
        "$ErrorActionPreference='Stop'; \
         Get-CimInstance Win32_Process -Filter '{filter}' | ForEach-Object {{ \
           $p=$_; \
           $s=Get-CimInstance Win32_Service -Filter \"ProcessId=$($p.ProcessId)\" | Select-Object Name,DisplayName; \
           [pscustomobject]@{{ \
             ProcessId=$p.ProcessId; \
             ExecutablePath=$p.ExecutablePath; \
             CommandLine=$p.CommandLine; \
             CreationDate=$p.CreationDate; \
             ThreadCount=$p.ThreadCount; \
             WorkingSetSize=$p.WorkingSetSize; \
             Services=@($s) \
           }} \
         }} | ConvertTo-Json -Depth 4 -Compress"
    );

    command_stdout("powershell.exe", &["-NoProfile", "-Command", &script])
        .ok()
        .map(|output| parse_windows_process_details_by_pid(&output))
        .unwrap_or_default()
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

fn collect_windows_process_details(pid: &str) -> Option<ProcessDetails> {
    if !pid.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    let script = format!(
        "$ErrorActionPreference='Stop'; \
         $p=Get-CimInstance Win32_Process -Filter 'ProcessId={pid}'; \
         if ($null -eq $p) {{ exit 1 }}; \
         $s=Get-CimInstance Win32_Service -Filter 'ProcessId={pid}' | Select-Object Name,DisplayName; \
         [pscustomobject]@{{ \
           Process=($p | Select-Object ExecutablePath,CommandLine,CreationDate,ThreadCount,WorkingSetSize); \
           Services=@($s) \
         }} | ConvertTo-Json -Depth 4 -Compress"
    );
    command_stdout("powershell.exe", &["-NoProfile", "-Command", &script])
        .ok()
        .and_then(|output| parse_windows_process_details(&output))
}

fn windows_process_filter(pids: &[String]) -> Option<String> {
    let mut numeric_pids = pids
        .iter()
        .filter(|pid| !pid.is_empty() && pid.chars().all(|ch| ch.is_ascii_digit()))
        .map(String::as_str)
        .collect::<Vec<_>>();
    numeric_pids.sort_unstable();
    numeric_pids.dedup();
    if numeric_pids.is_empty() {
        return None;
    }

    Some(
        numeric_pids
            .into_iter()
            .map(|pid| format!("ProcessId={pid}"))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

fn parse_windows_process_details_value(process: &Value) -> Option<ProcessDetails> {
    let executable = string_field(process, "ExecutablePath")
        .or_else(|| command_executable(string_field(process, "CommandLine").as_deref()));
    let command_line = string_field(process, "CommandLine");
    let started = string_field(process, "CreationDate").map(|date| format_windows_cim_date(&date));
    let threads = u64_field(process, "ThreadCount").and_then(|threads| threads.try_into().ok());
    let memory_rss_bytes = u64_field(process, "WorkingSetSize");
    let service = process
        .get("Services")
        .map(value_items)
        .unwrap_or_default()
        .into_iter()
        .filter_map(service_label)
        .collect::<Vec<_>>()
        .join(", ");
    let service = if service.is_empty() {
        None
    } else {
        Some(service)
    };

    let details = ProcessDetails {
        executable,
        command_line: command_line.clone(),
        service,
        working_dir: None,
        started,
        cpu_usage_tenths: None,
        memory_rss_bytes,
        threads,
        java: command_line.as_deref().and_then(parse_java_details),
    };

    if details == ProcessDetails::default() {
        None
    } else {
        Some(details)
    }
}

fn command_executable(command_line: Option<&str>) -> Option<String> {
    let command_line = command_line?.trim();
    if command_line.is_empty() {
        return None;
    }

    if let Some(rest) = command_line.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(rest[..end].to_string()).filter(|value| !value.is_empty());
    }

    command_line
        .split_whitespace()
        .next()
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

fn value_items(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(items) => items.iter().collect(),
        Value::Object(_) => vec![value],
        _ => Vec::new(),
    }
}

fn service_label(value: &Value) -> Option<String> {
    let name = string_field(value, "Name")?;
    let display_name = string_field(value, "DisplayName");
    match display_name {
        Some(display_name) if display_name != name => Some(format!("{name} ({display_name})")),
        _ => Some(name),
    }
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    match value.get(field)? {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
}

fn u64_field(value: &Value, field: &str) -> Option<u64> {
    match value.get(field)? {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn format_started_time(input: &str) -> String {
    chrono::NaiveDateTime::parse_from_str(input, "%a %b %e %H:%M:%S %Y")
        .map(|time| time.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| input.to_string())
}

fn format_windows_cim_date(input: &str) -> String {
    input
        .get(..14)
        .and_then(|date| chrono::NaiveDateTime::parse_from_str(date, "%Y%m%d%H%M%S").ok())
        .map(|time| time.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| input.to_string())
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

#[cfg(target_os = "windows")]
fn collect_windows_process_metrics() -> Option<ProcessMetrics> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    unsafe {
        let process = GetCurrentProcess();

        let mut memory = std::mem::zeroed::<PROCESS_MEMORY_COUNTERS>();
        if GetProcessMemoryInfo(
            process,
            &mut memory,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ) == 0
        {
            return None;
        }

        let mut creation = std::mem::zeroed::<FILETIME>();
        let mut exit = std::mem::zeroed::<FILETIME>();
        let mut kernel = std::mem::zeroed::<FILETIME>();
        let mut user = std::mem::zeroed::<FILETIME>();
        if GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) == 0 {
            return Some(ProcessMetrics {
                cpu_usage_tenths: None,
                memory_rss_bytes: Some(memory.WorkingSetSize as u64),
            });
        }

        let mut system_info = std::mem::zeroed::<SYSTEM_INFO>();
        GetSystemInfo(&mut system_info);
        let cpu_count = system_info.dwNumberOfProcessors;

        let current = WindowsMetricsSample {
            process_time_100ns: filetime_to_u64(kernel).saturating_add(filetime_to_u64(user)),
            system_time_100ns: monotonic_system_time_100ns(),
        };

        let previous = WINDOWS_METRICS_SAMPLE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .ok()
            .and_then(|mut guard| {
                let previous = *guard;
                *guard = Some(current);
                previous
            });

        let cpu_usage_tenths = previous.and_then(|previous| {
            windows_cpu_usage_tenths(
                previous.process_time_100ns,
                current.process_time_100ns,
                current
                    .system_time_100ns
                    .saturating_sub(previous.system_time_100ns),
                cpu_count,
            )
        });

        Some(ProcessMetrics {
            cpu_usage_tenths,
            memory_rss_bytes: Some(memory.WorkingSetSize as u64),
        })
    }
}

#[cfg(not(target_os = "windows"))]
fn collect_windows_process_metrics() -> Option<ProcessMetrics> {
    None
}

#[cfg(target_os = "windows")]
fn filetime_to_u64(filetime: windows_sys::Win32::Foundation::FILETIME) -> u64 {
    ((filetime.dwHighDateTime as u64) << 32) | filetime.dwLowDateTime as u64
}

#[cfg(target_os = "windows")]
fn monotonic_system_time_100ns() -> u64 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_nanos()
        .checked_div(100)
        .and_then(|value| value.try_into().ok())
        .unwrap_or(u64::MAX)
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
    fn parses_windows_process_details_from_cim_json() {
        let input = r#"
{
  "Process": {
    "ExecutablePath": "C:\\Windows\\System32\\svchost.exe",
    "CommandLine": "C:\\Windows\\System32\\svchost.exe -k LocalServiceNetworkRestricted -p",
    "CreationDate": "20260622091200.000000+540",
    "ThreadCount": 28,
    "WorkingSetSize": 73400320
  },
  "Services": [
    { "Name": "WinHttpAutoProxySvc", "DisplayName": "WinHTTP Web Proxy Auto-Discovery Service" },
    { "Name": "iphlpsvc", "DisplayName": "IP Helper" }
  ]
}
"#;

        let details = parse_windows_process_details(input).expect("details");

        assert_eq!(
            details.executable.as_deref(),
            Some("C:\\Windows\\System32\\svchost.exe")
        );
        assert_eq!(
            details.command_line.as_deref(),
            Some("C:\\Windows\\System32\\svchost.exe -k LocalServiceNetworkRestricted -p")
        );
        assert_eq!(
            details.service.as_deref(),
            Some("WinHttpAutoProxySvc (WinHTTP Web Proxy Auto-Discovery Service), iphlpsvc (IP Helper)")
        );
        assert_eq!(details.started.as_deref(), Some("2026-06-22 09:12"));
        assert_eq!(details.threads, Some(28));
        assert_eq!(details.memory_rss_bytes, Some(73_400_320));
    }

    #[test]
    fn parses_windows_process_details_by_pid_from_batch_cim_json() {
        let input = r#"
[
  {
    "ProcessId": 2460,
    "ExecutablePath": "C:\\Python312\\python.exe",
    "CommandLine": "python -m http.server 5050",
    "CreationDate": "20260622091200.000000+540",
    "ThreadCount": 7,
    "WorkingSetSize": 25165824,
    "Services": []
  },
  {
    "ProcessId": 980,
    "ExecutablePath": "C:\\Windows\\System32\\svchost.exe",
    "CommandLine": "C:\\Windows\\System32\\svchost.exe -k rpcss -p",
    "CreationDate": "20260622080000.000000+540",
    "ThreadCount": 18,
    "WorkingSetSize": 73400320,
    "Services": [
      { "Name": "RpcSs", "DisplayName": "Remote Procedure Call (RPC)" }
    ]
  }
]
"#;

        let details_by_pid = parse_windows_process_details_by_pid(input);

        let python = details_by_pid.get("2460").expect("python process");
        assert_eq!(
            python.executable.as_deref(),
            Some("C:\\Python312\\python.exe")
        );
        assert_eq!(
            python.command_line.as_deref(),
            Some("python -m http.server 5050")
        );
        assert_eq!(python.threads, Some(7));
        assert_eq!(python.memory_rss_bytes, Some(25_165_824));

        let service = details_by_pid.get("980").expect("service process");
        assert_eq!(
            service.service.as_deref(),
            Some("RpcSs (Remote Procedure Call (RPC))")
        );
        assert_eq!(service.started.as_deref(), Some("2026-06-22 08:00"));
    }

    #[test]
    fn windows_process_filter_keeps_only_unique_numeric_pids() {
        assert_eq!(
            windows_process_filter(&[
                "2460".to_string(),
                "980".to_string(),
                "2460".to_string(),
                "-".to_string(),
                "abc".to_string(),
            ])
            .as_deref(),
            Some("ProcessId=2460 OR ProcessId=980")
        );
    }

    #[test]
    fn parses_lsof_cwd_name_line() {
        assert_eq!(
            parse_lsof_cwd("p90759\nfcwd\nn/opt/tibero/monitor\n").as_deref(),
            Some("/opt/tibero/monitor")
        );
    }
}
