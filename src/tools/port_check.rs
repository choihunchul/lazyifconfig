use std::time::{Duration, Instant};

use tokio::net::TcpStream;
use tokio::time::timeout;

use super::{ToolInput, ToolResult, ToolResultSection};

pub async fn run(input: ToolInput, timeout_duration: Duration) -> Result<ToolResult, String> {
    let host = input.get("host").unwrap_or("").trim();
    let port_raw = input.get("port").unwrap_or("").trim();

    if host.is_empty() {
        return Err("Host is required.".to_string());
    }

    let port: u16 = port_raw
        .parse()
        .map_err(|_| "Port must be a number from 1 to 65535.".to_string())?;
    if port == 0 {
        return Err("Port must be a number from 1 to 65535.".to_string());
    }

    let address = format!("{host}:{port}");
    let start = Instant::now();
    let connect = timeout(timeout_duration, TcpStream::connect(address.as_str())).await;
    let elapsed = start.elapsed().as_millis();

    match connect {
        Ok(Ok(_stream)) => Ok(ToolResult {
            title: "Port Check".to_string(),
            sections: vec![
                ToolResultSection {
                    label: "Status".to_string(),
                    lines: vec!["OPEN".to_string()],
                },
                ToolResultSection {
                    label: "Latency".to_string(),
                    lines: vec![format!("{elapsed}ms")],
                },
            ],
            raw_output: format!(
                "$ lazyifconfig tools port-check {host} {port}\nOPEN in {elapsed}ms\n"
            ),
        }),
        Ok(Err(err)) => Ok(ToolResult {
            title: "Port Check".to_string(),
            sections: vec![
                ToolResultSection {
                    label: "Status".to_string(),
                    lines: vec!["CLOSED".to_string()],
                },
                ToolResultSection {
                    label: "Detail".to_string(),
                    lines: vec![err.to_string()],
                },
            ],
            raw_output: format!(
                "$ lazyifconfig tools port-check {host} {port}\nCLOSED after {elapsed}ms\n{err}\n"
            ),
        }),
        Err(_) => Ok(ToolResult {
            title: "Port Check".to_string(),
            sections: vec![
                ToolResultSection {
                    label: "Status".to_string(),
                    lines: vec!["ERROR".to_string()],
                },
                ToolResultSection {
                    label: "Detail".to_string(),
                    lines: vec![format!(
                        "Timed out after {}ms",
                        timeout_duration.as_millis()
                    )],
                },
            ],
            raw_output: format!(
                "$ lazyifconfig tools port-check {host} {port}\nTIMEOUT after {}ms\n",
                timeout_duration.as_millis()
            ),
        }),
    }
}
