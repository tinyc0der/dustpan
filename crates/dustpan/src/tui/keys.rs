use super::theme::Glyphs;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Up,
    Down,
    Open,
    Back,
    Jump(u8),
    Help,
    Settings,
    SwapPane,
    Quit,
    QuitNow,
    Text(char),
    Erase,
    Clear,
    Paste(String),
    Click { column: u16, row: u16 },
    Resize { width: u16, height: u16 },
}

/// What a key means. In a text field a letter is a letter, so `q`, `j`, `k` and the digits are
/// text and not commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Commands,
    Text,
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
    pub narrow_only: bool,
}

const fn hint(priority: u8, keys: &'static str, label: &'static str) -> Option<Hint> {
    Some(Hint {
        priority,
        keys,
        label,
    })
}

const fn doc(
    keys: &'static str,
    action: &'static str,
    hint: Option<Hint>,
    narrow_only: bool,
) -> KeyDoc {
    KeyDoc {
        keys,
        action,
        hint,
        narrow_only,
    }
}

const HELP: KeyDoc = doc("?", "Show or close this help", hint(1, "?", "Help"), false);
const QUIT: KeyDoc = doc("q", "Quit", hint(0, "q", "Quit"), false);
const QUIT_NOW: KeyDoc = doc("Ctrl+C", "Quit now", hint(6, "Ctrl+C", "Quit now"), false);

pub const HOME: [KeyDoc; 9] = [
    doc(
        "{up} {down} j k",
        "Move the cursor",
        hint(2, "{up}{down}", "Move"),
        false,
    ),
    doc(
        "Enter {right} l",
        "Open the entry",
        hint(3, "Enter", "Open"),
        false,
    ),
    doc("0..9", "Jump to an entry", hint(4, "0-9", "Jump"), false),
    doc(
        "Tab",
        "Swap list and detail below 100 columns",
        hint(2, "Tab", "Detail"),
        true,
    ),
    doc(",", "Settings", hint(5, ",", "Settings"), false),
    HELP,
    QUIT,
    QUIT_NOW,
    doc(
        "Wheel, click",
        "Wheel moves, click selects or opens",
        None,
        false,
    ),
];

pub const ABOUT: [KeyDoc; 6] = [
    doc(
        "Esc {left} Backspace",
        "Back to Home",
        hint(1, "Esc", "Back"),
        false,
    ),
    doc(
        "0 9",
        "Open Check a path or History now",
        hint(2, "0 9", "Try it now"),
        false,
    ),
    HELP,
    QUIT,
    QUIT_NOW,
    doc("Click Home", "Back to Home", None, false),
];

pub const HISTORY: [KeyDoc; 6] = [
    doc(
        "{up} {down} j k",
        "Scroll",
        hint(2, "{up}{down}", "Scroll"),
        false,
    ),
    doc(
        "Esc {left} Backspace",
        "Back to Home",
        hint(1, "Esc", "Back"),
        false,
    ),
    HELP,
    QUIT,
    QUIT_NOW,
    doc("Wheel, click Home", "Scroll, back to Home", None, false),
];

pub const CHECK: [KeyDoc; 8] = [
    doc("Enter", "Check the path", hint(1, "Enter", "Check"), false),
    doc("Esc", "Back to Home", hint(0, "Esc", "Back"), false),
    doc("Backspace", "Erase a letter", None, false),
    doc(
        "Ctrl+U",
        "Clear the field",
        hint(4, "Ctrl+U", "Clear"),
        false,
    ),
    doc(
        "{up} {down}",
        "Scroll the results",
        hint(3, "{up}{down}", "Scroll"),
        false,
    ),
    doc(
        "F1",
        "Show or close this help",
        hint(2, "F1", "Help"),
        false,
    ),
    QUIT_NOW,
    doc(
        "Paste",
        "Paste a path, or drag one in from Finder",
        None,
        false,
    ),
];

/// The screens that have their own key table, footer hints and help overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Help {
    Home,
    About,
    History,
    Check,
}

impl Help {
    pub fn name(self) -> &'static str {
        match self {
            Help::Home => "Home",
            Help::About => "About",
            Help::History => "History",
            Help::Check => "Check a path",
        }
    }

    pub fn docs(self) -> &'static [KeyDoc] {
        match self {
            Help::Home => &HOME,
            Help::About => &ABOUT,
            Help::History => &HISTORY,
            Help::Check => &CHECK,
        }
    }

    pub fn hints(self, width: u16) -> Vec<Hint> {
        let narrow = super::frame::is_narrow(width);
        self.docs()
            .iter()
            .filter(|doc| narrow || !doc.narrow_only)
            .filter_map(|doc| doc.hint)
            .collect()
    }
}

pub fn expand(template: &str, glyphs: &Glyphs) -> String {
    template
        .replace("{up}", glyphs.up)
        .replace("{down}", glyphs.down)
        .replace("{right}", glyphs.right)
        .replace("{left}", glyphs.left)
}

pub fn input(event: Event, focus: Focus) -> Option<Input> {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => match focus {
            Focus::Commands => command(key),
            Focus::Text => text(key),
        },
        Event::Paste(pasted) => Some(Input::Paste(pasted)),
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

fn command(key: KeyEvent) -> Option<Input> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return matches!(key.code, KeyCode::Char('c')).then_some(Input::QuitNow);
    }
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => Some(Input::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(Input::Down),
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => Some(Input::Open),
        KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => Some(Input::Back),
        KeyCode::Char(c @ '0'..='9') => Some(Input::Jump(c as u8 - b'0')),
        KeyCode::Char('?') | KeyCode::F(1) => Some(Input::Help),
        KeyCode::Char(',') => Some(Input::Settings),
        KeyCode::Tab => Some(Input::SwapPane),
        KeyCode::Char('q') => Some(Input::Quit),
        _ => None,
    }
}

fn text(key: KeyEvent) -> Option<Input> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Some(Input::QuitNow),
            KeyCode::Char('u') => Some(Input::Clear),
            _ => None,
        };
    }
    match key.code {
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::ALT) && !c.is_control() => {
            Some(Input::Text(c))
        }
        KeyCode::Backspace => Some(Input::Erase),
        KeyCode::Enter => Some(Input::Open),
        KeyCode::Esc => Some(Input::Back),
        KeyCode::Up => Some(Input::Up),
        KeyCode::Down => Some(Input::Down),
        KeyCode::F(1) => Some(Input::Help),
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

    fn command_key(code: KeyCode) -> Option<Input> {
        input(key(code, KeyModifiers::NONE), Focus::Commands)
    }

    fn text_key(code: KeyCode) -> Option<Input> {
        input(key(code, KeyModifiers::NONE), Focus::Text)
    }

    #[test]
    fn the_key_table_maps_every_alias() {
        assert_eq!(command_key(KeyCode::Char('j')), Some(Input::Down));
        assert_eq!(command_key(KeyCode::Down), Some(Input::Down));
        assert_eq!(command_key(KeyCode::Char('k')), Some(Input::Up));
        assert_eq!(command_key(KeyCode::Right), Some(Input::Open));
        assert_eq!(command_key(KeyCode::Esc), Some(Input::Back));
        assert_eq!(command_key(KeyCode::Backspace), Some(Input::Back));
        assert_eq!(command_key(KeyCode::Char('9')), Some(Input::Jump(9)));
        assert_eq!(command_key(KeyCode::Char('0')), Some(Input::Jump(0)));
        assert_eq!(command_key(KeyCode::Char('?')), Some(Input::Help));
        assert_eq!(command_key(KeyCode::F(1)), Some(Input::Help));
        assert_eq!(command_key(KeyCode::Char(',')), Some(Input::Settings));
        assert_eq!(command_key(KeyCode::Tab), Some(Input::SwapPane));
        assert_eq!(command_key(KeyCode::Char('x')), None);
    }

    #[test]
    fn in_a_text_field_q_j_k_and_digits_are_letters() {
        for c in ['q', 'j', 'k', 'h', 'l', '?', ',', '0', '7'] {
            assert_eq!(text_key(KeyCode::Char(c)), Some(Input::Text(c)), "{c}");
        }
        assert_eq!(text_key(KeyCode::Backspace), Some(Input::Erase));
        assert_eq!(text_key(KeyCode::Enter), Some(Input::Open));
        assert_eq!(text_key(KeyCode::Esc), Some(Input::Back));
        assert_eq!(text_key(KeyCode::F(1)), Some(Input::Help));
        assert_eq!(text_key(KeyCode::Tab), None);
    }

    #[test]
    fn control_c_quits_now_in_both_focuses_and_other_control_keys_do_nothing() {
        for focus in [Focus::Commands, Focus::Text] {
            assert_eq!(
                input(key(KeyCode::Char('c'), KeyModifiers::CONTROL), focus),
                Some(Input::QuitNow)
            );
            assert_eq!(
                input(key(KeyCode::Char('q'), KeyModifiers::CONTROL), focus),
                None
            );
        }
        assert_eq!(
            input(key(KeyCode::Char('u'), KeyModifiers::CONTROL), Focus::Text),
            Some(Input::Clear)
        );
        assert_eq!(
            input(
                key(KeyCode::Char('u'), KeyModifiers::CONTROL),
                Focus::Commands
            ),
            None
        );
    }

    #[test]
    fn a_paste_arrives_whole() {
        assert_eq!(
            input(Event::Paste("/a b".to_string()), Focus::Commands),
            Some(Input::Paste("/a b".to_string()))
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
        assert_eq!(input(release, Focus::Commands), None);
    }

    #[test]
    fn the_wheel_moves_and_a_left_click_carries_its_cell() {
        let wheel = |kind| input(mouse(kind), Focus::Commands);
        assert_eq!(wheel(MouseEventKind::ScrollUp), Some(Input::Up));
        assert_eq!(wheel(MouseEventKind::ScrollDown), Some(Input::Down));
        assert_eq!(
            wheel(MouseEventKind::Down(MouseButton::Left)),
            Some(Input::Click { column: 4, row: 7 })
        );
        assert_eq!(wheel(MouseEventKind::Down(MouseButton::Right)), None);
        assert_eq!(wheel(MouseEventKind::Moved), None);
    }
}
