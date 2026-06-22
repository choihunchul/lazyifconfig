use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

pub fn should_handle_key_event(key: KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) && !is_paste_shortcut(key)
}

fn is_paste_shortcut(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('v' | 'V')) && key.modifiers.contains(KeyModifiers::CONTROL)
}
