use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use lazyifconfig::input::{should_handle_key_event, InputBurstGuard};
use std::time::{Duration, Instant};

fn key_event(kind: KeyEventKind) -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char('j'),
        modifiers: KeyModifiers::NONE,
        kind,
        state: KeyEventState::NONE,
    }
}

#[test]
fn handles_key_presses_and_repeats_but_ignores_releases() {
    assert!(should_handle_key_event(key_event(KeyEventKind::Press)));
    assert!(should_handle_key_event(key_event(KeyEventKind::Repeat)));
    assert!(!should_handle_key_event(key_event(KeyEventKind::Release)));
}

#[test]
fn burst_guard_allows_normal_key_input() {
    let mut guard = InputBurstGuard::default();
    let start = Instant::now();

    for index in 0..InputBurstGuard::MAX_KEYS_PER_WINDOW {
        assert!(guard.should_handle_key(
            key_event(KeyEventKind::Press),
            start + Duration::from_millis(index as u64 * 20)
        ));
    }
}

#[test]
fn burst_guard_suppresses_fast_pasted_key_streams() {
    let mut guard = InputBurstGuard::default();
    let start = Instant::now();
    let key = key_event(KeyEventKind::Press);

    for index in 0..InputBurstGuard::MAX_KEYS_PER_WINDOW {
        assert!(guard.should_handle_key(key, start + Duration::from_millis(index as u64)));
    }

    assert!(!guard.should_handle_key(
        key,
        start + Duration::from_millis(InputBurstGuard::MAX_KEYS_PER_WINDOW as u64)
    ));
    assert!(!guard.should_handle_key(key, start + InputBurstGuard::SUPPRESS_DURATION / 2));
    let burst_detected_at =
        start + Duration::from_millis(InputBurstGuard::MAX_KEYS_PER_WINDOW as u64);
    assert!(guard.should_handle_key(
        key,
        burst_detected_at + InputBurstGuard::SUPPRESS_DURATION + Duration::from_millis(1)
    ));
}

#[test]
fn burst_guard_does_not_suppress_modified_shortcuts() {
    let mut guard = InputBurstGuard::default();
    let start = Instant::now();
    let mut key = key_event(KeyEventKind::Press);
    key.modifiers = KeyModifiers::CONTROL;

    for index in 0..(InputBurstGuard::MAX_KEYS_PER_WINDOW + 10) {
        assert!(guard.should_handle_key(key, start + Duration::from_millis(index as u64)));
    }
}
