use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use lazyifconfig::input::should_handle_key_event;

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
