use super::frame::{self, Header, MIN_HEIGHT, MIN_WIDTH};
use super::help;
use super::home::{self, ENTRIES, Facts};
use super::keys::{self, Input};
use super::theme::{self, Look};
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{cursor, execute};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

impl Size {
    fn rect(self) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }
}

/// What the screen shows. The overlay and the size decide it, so no two flags can disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Home,
    Help,
    TooSmall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quit {
    Normal,
    Now,
}

#[derive(Clone, Debug)]
pub struct State {
    pub look: Look,
    pub facts: Facts,
    pub size: Size,
    pub cursor: usize,
    pub help: bool,
    pub toast: Option<String>,
    pub quit: Option<Quit>,
}

impl State {
    pub fn new(look: Look, facts: Facts, size: Size) -> Self {
        State {
            look,
            facts,
            size,
            cursor: 0,
            help: false,
            toast: None,
            quit: None,
        }
    }

    pub fn mode(&self) -> Mode {
        if self.size.width < MIN_WIDTH || self.size.height < MIN_HEIGHT {
            Mode::TooSmall
        } else if self.help {
            Mode::Help
        } else {
            Mode::Home
        }
    }
}

pub fn update(mut state: State, input: Input) -> State {
    if let Input::Resize { width, height } = input {
        state.size = Size { width, height };
        return state;
    }
    state.toast = None;
    match (state.mode(), input) {
        (_, Input::QuitNow) => state.quit = Some(Quit::Now),
        (Mode::TooSmall, Input::Quit) => state.quit = Some(Quit::Normal),
        (Mode::TooSmall, _) => {}
        (Mode::Help, Input::Help | Input::Back | Input::Quit) => state.help = false,
        (Mode::Help, _) => {}
        (Mode::Home, input) => state = update_home(state, input),
    }
    state
}

fn update_home(mut state: State, input: Input) -> State {
    match input {
        Input::Up => state.cursor = state.cursor.saturating_sub(1),
        Input::Down => state.cursor = (state.cursor + 1).min(ENTRIES.len() - 1),
        Input::Open => {
            state.toast = Some(format!(
                "{}: {}",
                ENTRIES[state.cursor].label,
                ENTRIES[state.cursor].coming_in()
            ))
        }
        Input::Jump(key) => {
            if let Some(entry) = home::entry_for_key(key) {
                state.cursor = entry;
            }
        }
        Input::Click { column, row } => {
            let body = frame::regions(state.size.rect()).body;
            if let Some(entry) = home::entry_at(frame::panes(body).list, column, row) {
                state.cursor = entry;
            }
        }
        Input::Settings => state.toast = Some("Settings: Coming in M3".to_string()),
        Input::Help => state.help = true,
        Input::Quit => state.quit = Some(Quit::Normal),
        Input::Back | Input::QuitNow | Input::Resize { .. } => {}
    }
    state
}

pub fn view(state: &State, frame: &mut Frame) {
    if state.mode() == Mode::TooSmall {
        return too_small(state, frame);
    }
    let header = Header {
        breadcrumb: &["Home"],
        volume: state.facts.volume.as_ref(),
        admin: false,
    };
    let body = frame::draw(
        frame,
        &state.look,
        &header,
        state.toast.as_deref(),
        &keys::home_hints(),
    );
    home::render(frame, &state.look, &state.facts, state.cursor, body);
    if state.mode() == Mode::Help {
        help::render(frame, &state.look);
    }
}

fn too_small(state: &State, frame: &mut Frame) {
    let width = frame.area().width as usize;
    let ellipsis = state.look.glyphs.ellipsis;
    let lines = [
        format!("dustpan needs {MIN_WIDTH} by {MIN_HEIGHT}"),
        format!(
            "This window is {} by {}",
            state.size.width, state.size.height
        ),
        "Resize to continue".to_string(),
        "q quits".to_string(),
    ]
    .into_iter()
    .map(|line| Line::raw(frame::cut_end(&line, width, ellipsis)))
    .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), frame.area());
}

pub struct Options {
    pub ascii: bool,
    pub no_color: bool,
    pub interrupted_exit: i32,
}

/// How the session ended, for the caller to turn into an exit code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    Quit,
    Interrupted,
}

pub fn run(options: Options) -> std::io::Result<Finish> {
    let look = Look::new(
        theme::wants_ascii(options.ascii, |name| std::env::var(name).ok()),
        !options.no_color,
    );
    let facts = Facts {
        volume: dustpan_sys::boot_volume(),
    };
    install_panic_hook();
    let _restore = enter()?;
    watch_termination_signals(options.interrupted_exit);
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    let (width, height) = crossterm::terminal::size()?;
    let mut state = State::new(look, facts, Size { width, height });

    let mut dirty = true;
    loop {
        let (width, height) = crossterm::terminal::size()?;
        if (width, height) != (state.size.width, state.size.height) {
            state = update(state, Input::Resize { width, height });
            dirty = true;
        }
        if dirty {
            terminal.draw(|frame| view(&state, frame))?;
            dirty = false;
        }

        if event::poll(Duration::from_millis(100))? {
            dirty = true;
            loop {
                if let Some(input) = keys::from_event(event::read()?) {
                    state = update(state, input);
                }
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        match state.quit {
            Some(Quit::Normal) => return Ok(Finish::Quit),
            Some(Quit::Now) => return Ok(Finish::Interrupted),
            None => {}
        }
    }
}

/// A closed terminal leaves crossterm's `poll` spinning without returning, so no flag the loop checks would ever be read.
/// A watcher thread restores the terminal and exits as soon as SIGTERM, SIGHUP or SIGINT arrives.
fn watch_termination_signals(exit_code: i32) {
    #[cfg(unix)]
    {
        use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
        let Ok(mut signals) = signal_hook::iterator::Signals::new([SIGTERM, SIGHUP, SIGINT]) else {
            return;
        };
        std::thread::spawn(move || {
            if signals.forever().next().is_some() {
                restore();
                std::process::exit(exit_code);
            }
        });
    }
    #[cfg(not(unix))]
    let _ = exit_code;
}

/// Puts the terminal back on every way out of `run`, including an unwinding panic.
struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        restore();
    }
}

fn enter() -> std::io::Result<Restore> {
    enable_raw_mode()?;
    let guard = Restore;
    execute!(
        std::io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        cursor::Hide
    )?;
    Ok(guard)
}

fn restore() {
    let mut out = std::io::stdout();
    let _ = execute!(out, DisableMouseCapture, LeaveAlternateScreen, cursor::Show);
    let _ = disable_raw_mode();
}

/// The default hook prints on the alternate screen in raw mode, where the message is lost and garbled.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(width: u16, height: u16) -> State {
        State::new(
            Look::new(false, true),
            Facts::default(),
            Size { width, height },
        )
    }

    fn press(state: State, inputs: &[Input]) -> State {
        inputs
            .iter()
            .fold(state, |state, input| update(state, *input))
    }

    #[test]
    fn four_downs_and_one_k_land_on_entry_four() {
        let s = press(
            state(120, 40),
            &[
                Input::Down,
                Input::Down,
                Input::Down,
                Input::Down,
                Input::Up,
            ],
        );
        assert_eq!(ENTRIES[s.cursor].key, 4);
    }

    #[test]
    fn the_cursor_stops_at_both_ends() {
        let top = press(state(120, 40), &[Input::Up]);
        assert_eq!(top.cursor, 0);
        let bottom = press(state(120, 40), &[Input::Down; 20]);
        assert_eq!(bottom.cursor, ENTRIES.len() - 1);
    }

    #[test]
    fn enter_toasts_instead_of_opening() {
        let s = press(state(120, 40), &[Input::Down, Input::Open]);
        assert_eq!(s.toast.as_deref(), Some("Uninstall: Coming in M5"));
        assert_eq!(s.mode(), Mode::Home);
    }

    #[test]
    fn the_next_input_clears_the_toast_and_a_resize_keeps_it() {
        let s = press(state(120, 40), &[Input::Open]);
        let s = update(
            s,
            Input::Resize {
                width: 100,
                height: 30,
            },
        );
        assert!(s.toast.is_some());
        assert!(update(s, Input::Down).toast.is_none());
    }

    #[test]
    fn settings_toasts_coming_in_m3() {
        let s = press(state(120, 40), &[Input::Settings]);
        assert_eq!(s.toast.as_deref(), Some("Settings: Coming in M3"));
    }

    #[test]
    fn a_number_key_moves_the_cursor_to_its_entry() {
        let s = press(state(120, 40), &[Input::Jump(9)]);
        assert_eq!(ENTRIES[s.cursor].label, "History");
    }

    #[test]
    fn help_opens_and_closes_and_swallows_other_keys() {
        let open = press(state(120, 40), &[Input::Help]);
        assert_eq!(open.mode(), Mode::Help);
        let still = update(open.clone(), Input::Down);
        assert_eq!((still.mode(), still.cursor), (Mode::Help, 0));
        assert_eq!(update(open.clone(), Input::Help).mode(), Mode::Home);
        assert_eq!(update(open.clone(), Input::Back).mode(), Mode::Home);
        let closed_by_q = update(open, Input::Quit);
        assert_eq!((closed_by_q.mode(), closed_by_q.quit), (Mode::Home, None));
    }

    #[test]
    fn q_quits_from_home_and_ctrl_c_quits_from_anywhere() {
        assert_eq!(
            press(state(120, 40), &[Input::Quit]).quit,
            Some(Quit::Normal)
        );
        let help = press(state(120, 40), &[Input::Help, Input::QuitNow]);
        assert_eq!(help.quit, Some(Quit::Now));
        let small = press(state(50, 12), &[Input::QuitNow]);
        assert_eq!(small.quit, Some(Quit::Now));
    }

    #[test]
    fn below_sixty_by_sixteen_waits_for_a_resize_and_the_frame_returns() {
        let small = press(state(50, 12), &[Input::Down, Input::Open, Input::Help]);
        assert_eq!(small.mode(), Mode::TooSmall);
        assert_eq!(
            (small.cursor, small.toast.clone(), small.help),
            (0, None, false)
        );
        assert_eq!(
            press(small.clone(), &[Input::Quit]).quit,
            Some(Quit::Normal)
        );
        let back = update(
            small,
            Input::Resize {
                width: 120,
                height: 40,
            },
        );
        assert_eq!(back.mode(), Mode::Home);
        assert_eq!(state(60, 15).mode(), Mode::TooSmall);
        assert_eq!(state(59, 16).mode(), Mode::TooSmall);
        assert_eq!(state(60, 16).mode(), Mode::Home);
    }

    #[test]
    fn help_survives_a_trip_through_too_small() {
        let s = press(state(120, 40), &[Input::Help]);
        let s = update(
            s,
            Input::Resize {
                width: 40,
                height: 10,
            },
        );
        assert_eq!(s.mode(), Mode::TooSmall);
        let s = update(
            s,
            Input::Resize {
                width: 120,
                height: 40,
            },
        );
        assert_eq!(s.mode(), Mode::Help);
    }

    #[test]
    fn a_click_selects_the_row_under_it() {
        let s = state(80, 24);
        let list = frame::panes(frame::regions(s.size.rect()).body).list;
        let s = update(
            s,
            Input::Click {
                column: list.x + 4,
                row: list.y + 3,
            },
        );
        assert_eq!(s.cursor, 2);
    }
}
