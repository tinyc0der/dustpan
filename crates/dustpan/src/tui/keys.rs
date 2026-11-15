use super::theme::Glyphs;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Up,
    Down,
    Open,
    Back,
    Jump(u8),
    Help,
    Settings,
    Quit,
    QuitNow,
    Click { column: u16, row: u16 },
    Resize { width: u16, height: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hint {
    pub priority: u8,
    pub keys: &'static str,
    pub label: &'static str,
}

impl Hint {
    pub fn keys(&self, glyphs: &Glyphs) -> String {
        expand(self.keys, glyphs)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct KeyDoc {
    pub keys: &'static str,
    pub action: &'static str,
    pub hint: Option<Hint>,
}

const fn hint(priority: u8, keys: &'static str, label: &'static str) -> Option<Hint> {
    Some(Hint {
        priority,
        keys,
        label,
    })
}

pub const HOME: [KeyDoc; 8] = [
    KeyDoc {
        keys: "{up} {down} j k",
        action: "Move the cursor",
        hint: hint(2, "{up}{down}", "Move"),
    },
    KeyDoc {
        keys: "Enter {right} l",
        action: "Open the entry",
        hint: hint(3, "Enter", "Open"),
    },
    KeyDoc {
        keys: "1..9",
        action: "Jump to an entry",
        hint: hint(4, "1-9", "Jump"),
    },
    KeyDoc {
        keys: ",",
        action: "Settings",
        hint: hint(5, ",", "Settings"),
    },
    KeyDoc {
        keys: "?",
        action: "Show or close this help",
        hint: hint(1, "?", "Help"),
    },
    KeyDoc {
        keys: "q",
        action: "Quit",
        hint: hint(0, "q", "Quit"),
    },
    KeyDoc {
        keys: "Ctrl+C",
        action: "Quit now",
        hint: hint(6, "Ctrl+C", "Quit now"),
    },
    KeyDoc {
        keys: "Wheel, click",
        action: "Move, select a row",
        hint: None,
    },
];

pub fn home_hints() -> Vec<Hint> {
    HOME.iter().filter_map(|doc| doc.hint).collect()
}

pub fn expand(template: &str, glyphs: &Glyphs) -> String {
    template
        .replace("{up}", glyphs.up)
        .replace("{down}", glyphs.down)
        .replace("{right}", glyphs.right)
}

pub fn from_event(event: Event) -> Option<Input> {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => from_key(key),
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => Some(Input::Up),
            MouseEventKind::ScrollDown => Some(Input::Down),
            MouseEventKind::Down(MouseButton::Left) => Some(Input::Click {
                column: mouse.column,
                row: mouse.row,
            }),
            _ => None,
        },
        Event::Resize(width, height) => Some(Input::Resize { width, height }),
        _ => None,
    }
}

fn from_key(key: KeyEvent) -> Option<Input> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return matches!(key.code, KeyCode::Char('c')).then_some(Input::QuitNow);
    }
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => Some(Input::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(Input::Down),
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => Some(Input::Open),
        KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => Some(Input::Back),
        KeyCode::Char(c @ '1'..='9') => Some(Input::Jump(c as u8 - b'0')),
        KeyCode::Char('?') => Some(Input::Help),
        KeyCode::Char(',') => Some(Input::Settings),
        KeyCode::Char('q') => Some(Input::Quit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventState, MouseEvent};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn mouse(kind: MouseEventKind) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column: 4,
            row: 7,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn the_key_table_maps_every_alias() {
        let plain = |code| from_event(key(code, KeyModifiers::NONE));
        assert_eq!(plain(KeyCode::Char('j')), Some(Input::Down));
        assert_eq!(plain(KeyCode::Down), Some(Input::Down));
        assert_eq!(plain(KeyCode::Char('k')), Some(Input::Up));
        assert_eq!(plain(KeyCode::Right), Some(Input::Open));
        assert_eq!(plain(KeyCode::Esc), Some(Input::Back));
        assert_eq!(plain(KeyCode::Char('9')), Some(Input::Jump(9)));
        assert_eq!(plain(KeyCode::Char('0')), None);
        assert_eq!(plain(KeyCode::Char('?')), Some(Input::Help));
        assert_eq!(plain(KeyCode::Char(',')), Some(Input::Settings));
        assert_eq!(plain(KeyCode::Char('x')), None);
    }

    #[test]
    fn control_c_quits_now_and_other_control_keys_do_nothing() {
        assert_eq!(
            from_event(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Input::QuitNow)
        );
        assert_eq!(
            from_event(key(KeyCode::Char('q'), KeyModifiers::CONTROL)),
            None
        );
    }

    #[test]
    fn key_release_is_ignored() {
        let release = Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        });
        assert_eq!(from_event(release), None);
    }

    #[test]
    fn the_wheel_moves_and_a_left_click_carries_its_cell() {
        assert_eq!(from_event(mouse(MouseEventKind::ScrollUp)), Some(Input::Up));
        assert_eq!(
            from_event(mouse(MouseEventKind::ScrollDown)),
            Some(Input::Down)
        );
        assert_eq!(
            from_event(mouse(MouseEventKind::Down(MouseButton::Left))),
            Some(Input::Click { column: 4, row: 7 })
        );
        assert_eq!(
            from_event(mouse(MouseEventKind::Down(MouseButton::Right))),
            None
        );
        assert_eq!(from_event(mouse(MouseEventKind::Moved)), None);
    }
}
