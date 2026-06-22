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

fn key_code_event(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
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
fn ignores_ctrl_v_paste_shortcut() {
    let mut key = key_code_event(KeyCode::Char('v'));
    key.modifiers = KeyModifiers::CONTROL;

    assert!(!should_handle_key_event(key));
}

#[test]
fn handles_arrow_keys() {
    assert!(should_handle_key_event(key_code_event(KeyCode::Up)));
    assert!(should_handle_key_event(key_code_event(KeyCode::Down)));
    assert!(should_handle_key_event(key_code_event(KeyCode::Left)));
    assert!(should_handle_key_event(key_code_event(KeyCode::Right)));
}
