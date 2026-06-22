use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub fn should_handle_key_event(key: KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

#[derive(Debug)]
pub struct InputBurstGuard {
    recent_key_times: VecDeque<Instant>,
    suppress_until: Option<Instant>,
}

impl Default for InputBurstGuard {
    fn default() -> Self {
        Self {
            recent_key_times: VecDeque::with_capacity(Self::MAX_KEYS_PER_WINDOW + 1),
            suppress_until: None,
        }
    }
}

impl InputBurstGuard {
    pub const MAX_KEYS_PER_WINDOW: usize = 32;
    pub const WINDOW: Duration = Duration::from_millis(120);
    pub const SUPPRESS_DURATION: Duration = Duration::from_millis(600);

    pub fn should_handle_key(&mut self, key: KeyEvent, now: Instant) -> bool {
        if !is_burst_candidate(key) {
            return true;
        }

        if let Some(until) = self.suppress_until {
            if now < until {
                return false;
            }
            self.suppress_until = None;
            self.recent_key_times.clear();
        }

        while self
            .recent_key_times
            .front()
            .is_some_and(|seen_at| now.duration_since(*seen_at) > Self::WINDOW)
        {
            self.recent_key_times.pop_front();
        }

        if self.recent_key_times.len() >= Self::MAX_KEYS_PER_WINDOW {
            self.suppress_until = Some(now + Self::SUPPRESS_DURATION);
            self.recent_key_times.clear();
            return false;
        }

        self.recent_key_times.push_back(now);
        true
    }
}

fn is_burst_candidate(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char(_))
        && !key.modifiers.intersects(
            KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::META,
        )
}
