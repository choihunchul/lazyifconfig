use crossterm::event::{KeyEvent, KeyEventKind};

pub fn should_handle_key_event(key: KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}
