use std::collections::BTreeMap;
use std::time::Duration;

use lazyifconfig::tools::{port_check, ToolInput};
use tokio::net::TcpListener;

fn input(values: &[(&str, &str)]) -> ToolInput {
    ToolInput {
        values: values
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect::<BTreeMap<_, _>>(),
    }
}

#[tokio::test]
async fn port_check_reports_open_local_listener() {
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(err) => panic!("failed to bind local listener: {err}"),
    };
    let port = listener.local_addr().unwrap().port().to_string();
    let accept_task = tokio::spawn(async move {
        let _ = listener.accept().await;
    });

    let result = port_check::run(
        input(&[("host", "127.0.0.1"), ("port", &port)]),
        Duration::from_secs(1),
    )
    .await
    .unwrap();

    assert_eq!(result.title, "Port Check");
    assert!(result.sections.iter().any(|section| {
        section.label == "Status" && section.lines.iter().any(|line| line.contains("OPEN"))
    }));
    assert!(result
        .raw_output
        .contains("lazyifconfig tools port-check 127.0.0.1"));

    let _ = accept_task.await;
}

#[tokio::test]
async fn port_check_rejects_invalid_port() {
    let err = port_check::run(
        input(&[("host", "127.0.0.1"), ("port", "not-a-port")]),
        Duration::from_millis(50),
    )
    .await
    .unwrap_err();

    assert!(err.contains("Port must be a number"));
}

#[test]
fn dns_command_candidates_prefer_dig() {
    let candidates = lazyifconfig::tools::dns::command_candidates("example.com");

    assert_eq!(candidates[0].program, "dig");
    assert_eq!(candidates[0].args, vec!["example.com"]);
    assert_eq!(candidates[1].program, "host");
    assert_eq!(candidates[2].program, "nslookup");
}

#[test]
fn ping_command_uses_small_count_per_platform() {
    let mac = lazyifconfig::tools::ping::command_spec_for_os("macos", "8.8.8.8");
    let linux = lazyifconfig::tools::ping::command_spec_for_os("linux", "8.8.8.8");

    assert_eq!(mac.program, "ping");
    assert_eq!(mac.args, vec!["-c", "4", "8.8.8.8"]);
    assert_eq!(linux.program, "ping");
    assert_eq!(linux.args, vec!["-c", "4", "8.8.8.8"]);
}
