use crate::app::App;
use crate::command::{run_command_capture, CommandResult};
use crate::model::{CommandOutput, CommandSourceId, EventSeverity, NetworkEvent, NetworkEventKind};
use crate::update::{self, CheckOutcome, UpdateMessage, UpdateStatus};
use std::time::Duration;

const RELEASE_CHECK_INTERVAL_SECS: u64 = 6 * 60 * 60;

pub fn maybe_start_auto_update_check(app: &mut App) {
    let is_busy = matches!(
        app.update_status,
        UpdateStatus::Checking { .. } | UpdateStatus::Installing { .. }
    );
    if is_busy {
        return;
    }

    let should_check = match app.last_update_check {
        None => true,
        Some(last) => last.elapsed() >= Duration::from_secs(RELEASE_CHECK_INTERVAL_SECS),
    };

    if should_check {
        start_update_check(app, false);
    }
}

pub fn maybe_start_auto_update_install(app: &mut App) {
    let Some(update) = app.pending_update.clone() else {
        return;
    };

    let is_busy = matches!(
        app.update_status,
        UpdateStatus::Checking { .. } | UpdateStatus::Installing { .. }
    );
    if is_busy {
        return;
    }

    if app.attempted_update_version.as_deref() == Some(update.target_version.as_str()) {
        return;
    }

    start_update_install(app, false);
}

pub fn start_update_check(app: &mut App, manual: bool) {
    let is_busy = matches!(
        app.update_status,
        UpdateStatus::Checking { .. } | UpdateStatus::Installing { .. }
    );
    if is_busy {
        return;
    }

    let Ok(url) = update::release_api_url() else {
        app.update_status = UpdateStatus::Error {
            message: "invalid GitHub repository URL".to_string(),
        };
        app.push_event(NetworkEvent::new(
            NetworkEventKind::UpdateCheckFailed,
            EventSeverity::Error,
            "Update check failed: invalid GitHub repository URL".to_string(),
        ));
        return;
    };

    app.update_status = UpdateStatus::Checking { manual };
    app.last_update_check = Some(std::time::Instant::now());

    let update_messages = app.update_messages.clone();
    let async_outputs = app.async_command_outputs.clone();
    tokio::spawn(async move {
        let started_at = std::time::SystemTime::now();
        let capture = run_command_capture(
            "curl",
            &[
                "-sS",
                "-L",
                "-m",
                "10",
                "-H",
                "Accept: application/vnd.github+json",
                "-H",
                "User-Agent: lazyifconfig",
                &url,
            ],
        );

        if let Ok(mut lock) = async_outputs.lock() {
            lock.insert(
                CommandSourceId::GitHubRelease,
                CommandOutput {
                    command: format!(
                        "curl -sS -L -m 10 -H 'Accept: application/vnd.github+json' -H 'User-Agent: lazyifconfig' {url}"
                    ),
                    stdout: capture.as_ref().map(|out| out.stdout.clone()).unwrap_or_default(),
                    stderr: capture
                        .as_ref()
                        .map(|out| out.stderr.clone())
                        .unwrap_or_else(|err| err.clone()),
                    executed_at: started_at,
                    exit_code: capture.as_ref().ok().and_then(|out| out.exit_code).or(Some(1)),
                },
            );
        }

        let result = capture
            .and_then(|out| command_stdout(&out))
            .and_then(|stdout| update::evaluate_release_json(&stdout));

        if let Ok(mut lock) = update_messages.lock() {
            lock.push(UpdateMessage::CheckFinished { manual, result });
        }
    });
}

pub fn start_update_install(app: &mut App, manual: bool) {
    let Some(update) = app.pending_update.clone() else {
        if manual {
            app.push_event(NetworkEvent::new(
                NetworkEventKind::UpdateCheckFailed,
                EventSeverity::Warning,
                "No pending update found. Press 'u' to check now.".to_string(),
            ));
        }
        return;
    };

    let is_busy = matches!(
        app.update_status,
        UpdateStatus::Checking { .. } | UpdateStatus::Installing { .. }
    );
    if is_busy {
        return;
    }

    app.attempted_update_version = Some(update.target_version.clone());
    app.update_status = UpdateStatus::Installing {
        version: update.target_version.clone(),
        manual,
    };

    let update_messages = app.update_messages.clone();
    tokio::spawn(async move {
        let current_exe = std::env::current_exe().map_err(|e| e.to_string());
        let result = match current_exe {
            Ok(path) => update::install_update(&update, &path),
            Err(err) => Err(err),
        };

        if let Ok(mut lock) = update_messages.lock() {
            lock.push(UpdateMessage::InstallFinished {
                manual,
                version: update.target_version.clone(),
                result,
            });
        }
    });
}

pub fn drain_update_messages(app: &mut App) {
    let messages = if let Ok(mut lock) = app.update_messages.lock() {
        lock.drain(..).collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    for message in messages {
        match message {
            UpdateMessage::CheckFinished { manual, result } => match result {
                Ok(CheckOutcome::UpToDate {
                    current_version: _,
                    release_date,
                }) => {
                    app.pending_update = None;
                    app.latest_release_date = release_date;
                    app.update_status = UpdateStatus::UpToDate;
                    if manual {
                        app.push_event(NetworkEvent::new(
                            NetworkEventKind::UpdateInstalled,
                            EventSeverity::Info,
                            "Already running the latest release.".to_string(),
                        ));
                    }
                }
                Ok(CheckOutcome::Available(update)) => {
                    let version = update.target_version.clone();
                    app.latest_release_date = Some(update.release_date.clone());
                    app.pending_update = Some(update);
                    app.update_status = UpdateStatus::Available {
                        version: version.clone(),
                    };
                    app.push_event(NetworkEvent::new(
                        NetworkEventKind::UpdateAvailable,
                        EventSeverity::Info,
                        if manual {
                            format!("Update available: v{version}. Starting install.")
                        } else {
                            format!("Auto-update found v{version}. Starting install.")
                        },
                    ));
                }
                Err(err) => {
                    app.update_status = UpdateStatus::Error {
                        message: err.clone(),
                    };
                    app.push_event(NetworkEvent::new(
                        NetworkEventKind::UpdateCheckFailed,
                        EventSeverity::Error,
                        format!("Update check failed: {err}"),
                    ));
                }
            },
            UpdateMessage::InstallFinished {
                version, result, ..
            } => match result {
                Ok(()) => {
                    app.pending_update = None;
                    app.update_status = UpdateStatus::Updated {
                        version: version.clone(),
                    };
                    app.push_event(NetworkEvent::new(
                        NetworkEventKind::UpdateInstalled,
                        EventSeverity::Info,
                        format!("Updated binary to v{version}. Restart lazyifconfig to use it."),
                    ));
                }
                Err(err) => {
                    app.update_status = UpdateStatus::Error {
                        message: err.clone(),
                    };
                    app.push_event(NetworkEvent::new(
                        NetworkEventKind::UpdateCheckFailed,
                        EventSeverity::Error,
                        format!("Update install failed for v{version}: {err}"),
                    ));
                }
            },
        }
    }
}

fn command_stdout(output: &CommandResult) -> Result<String, String> {
    if output.exit_code == Some(0) {
        Ok(output.stdout.clone())
    } else if output.stderr.trim().is_empty() {
        Err(format!("command exited with {:?}", output.exit_code))
    } else {
        Err(output.stderr.clone())
    }
}
