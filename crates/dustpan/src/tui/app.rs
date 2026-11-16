use super::about;
use super::check::{self, CheckView};
use super::facts::{Environment, Facts};
use super::frame::{self, Header, MIN_HEIGHT, MIN_WIDTH, Pane};
use super::help;
use super::history::{self, HistoryView};
use super::home::{self, ENTRIES, Kind, Tool};
use super::keys::{self, Focus, Input};
use super::theme::{self, Look};
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
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
use std::sync::Mutex;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Home,
    About,
    History,
    Check,
    Help,
    TooSmall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quit {
    Normal,
    Now,
}

/// Where the user is. Every screen but Home goes back to Home, which keeps its cursor.
#[derive(Clone, Debug)]
pub enum Screen {
    Home,
    /// The page of a planned entry, by its index in `ENTRIES`.
    About(usize),
    History(HistoryView),
    Check(CheckView),
}

#[derive(Clone, Debug)]
pub struct State {
    pub look: Look,
    pub facts: Facts,
    pub size: Size,
    pub cursor: usize,
    pub pane: Pane,
    pub screen: Screen,
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
            pane: Pane::List,
            screen: Screen::Home,
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
            match self.screen {
                Screen::Home => Mode::Home,
                Screen::About(_) => Mode::About,
                Screen::History(_) => Mode::History,
                Screen::Check(_) => Mode::Check,
            }
        }
    }

    /// Only the Check field takes text. With help or the too-small message over it, keys are
    /// commands again, so `?` and `q` close what covers it.
    pub fn focus(&self) -> Focus {
        match self.mode() {
            Mode::Check => Focus::Text,
            _ => Focus::Commands,
        }
    }

    fn help_for(&self) -> keys::Help {
        match self.screen {
            Screen::Home => keys::Help::Home,
            Screen::About(_) => keys::Help::About,
            Screen::History(_) => keys::Help::History,
            Screen::Check(_) => keys::Help::Check,
        }
    }

    fn breadcrumb(&self) -> Vec<&'static str> {
        match self.screen {
            Screen::Home => vec!["Home"],
            Screen::About(index) => vec!["Home", ENTRIES[index].label],
            Screen::History(_) => vec!["Home", keys::Help::History.name()],
            Screen::Check(_) => vec!["Home", keys::Help::Check.name()],
        }
    }

    fn body(&self) -> Rect {
        frame::regions(self.size.rect()).body
    }
}

pub fn update(mut state: State, input: Input) -> State {
    if let Input::Resize { width, height } = input {
        state.size = Size { width, height };
        return settle(state);
    }
    state.toast = None;
    match (state.mode(), &input) {
        (_, Input::QuitNow) => state.quit = Some(Quit::Now),
        (Mode::TooSmall, Input::Quit) => state.quit = Some(Quit::Normal),
        (Mode::TooSmall, _) => {}
        (Mode::Help, Input::Help | Input::Back | Input::Quit) => state.help = false,
        (Mode::Help, _) => {}
        (_, Input::Help) => state.help = true,
        (_, Input::Click { column, row }) if clicks_home_crumb(&state, *column, *row) => {
            state.screen = Screen::Home;
        }
        (Mode::Home, _) => state = update_home(state, input),
        (Mode::About, _) => state = update_about(state, input),
        (Mode::History, _) => state = update_history(state, input),
        (Mode::Check, _) => state = update_check(state, input),
    }
    settle(state)
}

fn clicks_home_crumb(state: &State, column: u16, row: u16) -> bool {
    !matches!(state.screen, Screen::Home)
        && row == 0
        && frame::crumb_columns(&state.look, &state.breadcrumb(), 0)
            .is_some_and(|columns| columns.contains(&column))
}

/// A scroll offset never points past the last row, whatever the window did.
fn settle(mut state: State) -> State {
    let body = state.body();
    let look = state.look;
    match &mut state.screen {
        Screen::History(view) => {
            let most = history::max_scroll(&look, view, body.width as usize, body.height);
            view.scroll = view.scroll.min(most);
        }
        Screen::Check(view) => {
            let most = check::max_scroll(&look, view, body);
            view.scroll = view.scroll.min(most);
        }
        Screen::Home | Screen::About(_) => {}
    }
    state
}

fn open(mut state: State, entry: usize) -> State {
    state.cursor = entry;
    state.screen = match ENTRIES[entry].kind {
        Kind::Ready(Tool::History) => Screen::History(HistoryView::open(&state.facts.env)),
        Kind::Ready(Tool::Check) => Screen::Check(CheckView::open(&state.facts.env)),
        Kind::Planned { .. } => Screen::About(entry),
    };
    state
}

fn update_home(mut state: State, input: Input) -> State {
    match input {
        Input::Up => state.cursor = state.cursor.saturating_sub(1),
        Input::Down => state.cursor = (state.cursor + 1).min(ENTRIES.len() - 1),
        Input::Open => {
            let entry = state.cursor;
            return open(state, entry);
        }
        Input::Jump(key) => {
            if let Some(entry) = home::entry_for_key(key) {
                state.cursor = entry;
            }
        }
        Input::Click { column, row } => {
            let list = frame::panes(state.body(), state.pane).list;
            match list.and_then(|list| home::entry_at(list, column, row)) {
                Some(entry) if entry == state.cursor => return open(state, entry),
                Some(entry) => state.cursor = entry,
                None => {}
            }
        }
        Input::Settings => state.toast = Some("Settings: arrives in M3".to_string()),
        Input::SwapPane => {
            if frame::is_narrow(state.size.width) {
                state.pane = state.pane.swapped();
            }
        }
        Input::Quit => state.quit = Some(Quit::Normal),
        _ => {}
    }
    state
}

fn update_about(mut state: State, input: Input) -> State {
    match input {
        Input::Back => state.screen = Screen::Home,
        Input::Jump(key) => {
            if let Some(entry) = home::entry_for_key(key)
                && matches!(ENTRIES[entry].kind, Kind::Ready(_))
            {
                return open(state, entry);
            }
        }
        Input::Quit => state.quit = Some(Quit::Normal),
        _ => {}
    }
    state
}

fn update_history(mut state: State, input: Input) -> State {
    match (&mut state.screen, input) {
        (Screen::History(view), Input::Up) => view.scroll = view.scroll.saturating_sub(1),
        (Screen::History(view), Input::Down) => view.scroll += 1,
        (_, Input::Back) => state.screen = Screen::Home,
        (_, Input::Quit) => state.quit = Some(Quit::Normal),
        _ => {}
    }
    state
}

fn update_check(mut state: State, input: Input) -> State {
    let env: Environment = state.facts.env.clone();
    let Screen::Check(view) = &mut state.screen else {
        return state;
    };
    match input {
        Input::Text(c) => view.type_char(c),
        Input::Paste(pasted) => view.paste(&pasted),
        Input::Erase => view.erase(),
        Input::Clear => view.clear(),
        Input::Open => view.submit(&env),
        Input::Up => view.scroll = view.scroll.saturating_sub(1),
        Input::Down => view.scroll += 1,
        Input::Back => state.screen = Screen::Home,
        _ => {}
    }
    state
}

pub fn view(state: &State, frame: &mut Frame) {
    if state.mode() == Mode::TooSmall {
        return too_small(state, frame);
    }
    let crumbs = state.breadcrumb();
    let header = Header {
        breadcrumb: &crumbs,
        volume: state.facts.volume.as_ref(),
        admin: false,
    };
    let help = state.help_for();
    let body = frame::draw_chrome(
        frame,
        &state.look,
        &header,
        state.toast.as_deref(),
        &help.hints(state.size.width),
    );
    match &state.screen {
        Screen::Home => home::render(frame, &state.look, state.cursor, state.pane, body),
        Screen::About(index) => about::render(frame, &state.look, *index, body),
        Screen::History(view) => history::render(frame, &state.look, view, body),
        Screen::Check(view) => check::render(frame, &state.look, view, body),
    }
    if state.mode() == Mode::Help {
        help::render(frame, &state.look, help);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    Quit,
    Interrupted,
}

pub fn run(options: Options) -> std::io::Result<Finish> {
    let look = Look::new(
        theme::wants_ascii(options.ascii, dustpan_sys::console_is_utf8(), |name| {
            std::env::var(name).ok()
        }),
        !options.no_color,
    );
    let facts = Facts {
        volume: dustpan_sys::boot_volume(),
        env: Environment::process(),
    };
    install_panic_hook();
    let _restore = enter()?;
    watch_termination(options.interrupted_exit);
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
            let _drawing = DRAWING
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            terminal.draw(|frame| view(&state, frame))?;
            dirty = false;
        }

        if event::poll(Duration::from_millis(100))? {
            dirty = true;
            loop {
                if let Some(input) = keys::input(event::read()?, state.focus()) {
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

/// The termination path takes this and never releases it, so a signal cannot land mid-frame and leave the cursor hidden.
static DRAWING: Mutex<()> = Mutex::new(());
static RESTORED: Mutex<bool> = Mutex::new(false);

fn watch_termination(exit_code: i32) {
    let _ = dustpan_sys::on_termination(move || {
        let _drawing = DRAWING
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        restore();
        std::process::exit(exit_code);
    });
}

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
        EnableBracketedPaste,
        cursor::Hide
    )?;
    Ok(guard)
}

fn restore() {
    once(&RESTORED, || {
        let mut out = std::io::stdout();
        let _ = execute!(
            out,
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen,
            cursor::Show
        );
        let _ = disable_raw_mode();
    });
}

fn once(done: &Mutex<bool>, action: impl FnOnce()) {
    let mut done = done.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if !std::mem::replace(&mut *done, true) {
        action();
    }
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

    fn fixture_env(home: &std::path::Path) -> Environment {
        let home = home.to_string_lossy().into_owned();
        Environment::fixed(&[
            ("HOME", home.clone()),
            ("USERPROFILE", home.clone()),
            ("APPDATA", format!("{home}/AppData/Roaming")),
            ("LOCALAPPDATA", format!("{home}/AppData/Local")),
        ])
    }

    fn state(width: u16, height: u16) -> State {
        let home = std::env::temp_dir().join("dustpan-no-such-home");
        State::new(
            Look::new(false, true),
            Facts {
                volume: None,
                env: fixture_env(&home),
            },
            Size { width, height },
        )
    }

    fn press(state: State, inputs: &[Input]) -> State {
        inputs.iter().cloned().fold(state, update)
    }

    fn entry_with(label: &str) -> usize {
        ENTRIES.iter().position(|e| e.label == label).unwrap()
    }

    fn type_text(state: State, text: &str) -> State {
        text.chars()
            .fold(state, |state, c| update(state, Input::Text(c)))
    }

    #[test]
    fn restoring_twice_runs_the_restore_once() {
        let done = Mutex::new(false);
        let mut runs = 0;
        once(&done, || runs += 1);
        once(&done, || runs += 1);
        assert_eq!(runs, 1);
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
        let bottom = press(state(120, 40), &vec![Input::Down; 20]);
        assert_eq!(bottom.cursor, ENTRIES.len() - 1);
    }

    #[test]
    fn enter_on_a_planned_entry_opens_its_page_and_back_returns_to_the_same_row() {
        let s = press(state(120, 40), &[Input::Down, Input::Open]);
        assert_eq!(s.mode(), Mode::About);
        assert!(matches!(s.screen, Screen::About(1)));
        for back in [Input::Back, Input::Back] {
            let home = update(s.clone(), back);
            assert_eq!((home.mode(), home.cursor), (Mode::Home, 1));
        }
    }

    #[test]
    fn enter_on_a_ready_entry_opens_its_screen_and_back_keeps_the_cursor() {
        let history = press(state(120, 40), &[Input::Jump(9), Input::Open]);
        assert_eq!(history.mode(), Mode::History);
        let back = update(history, Input::Back);
        assert_eq!(ENTRIES[back.cursor].label, "History");
        assert_eq!(back.mode(), Mode::Home);
        let check = press(state(120, 40), &[Input::Jump(0), Input::Open]);
        assert_eq!(check.mode(), Mode::Check);
        let back = update(check, Input::Back);
        assert_eq!(ENTRIES[back.cursor].label, "Check a path");
    }

    #[test]
    fn a_click_on_the_selected_row_opens_it_and_a_click_on_another_only_selects() {
        let s = state(80, 24);
        let list = frame::panes(s.body(), s.pane).list.unwrap();
        let row_of = |entry: usize| list.y + 2 + entry as u16;
        let click = |s: State, entry: usize| {
            update(
                s,
                Input::Click {
                    column: list.x + 4,
                    row: row_of(entry),
                },
            )
        };
        let selected = click(s, 2);
        assert_eq!((selected.cursor, selected.mode()), (2, Mode::Home));
        let opened = click(selected, 2);
        assert_eq!(opened.mode(), Mode::About);
    }

    #[test]
    fn a_click_on_the_home_crumb_goes_back_and_a_click_elsewhere_on_the_header_does_not() {
        let s = press(state(80, 24), &[Input::Down, Input::Open]);
        let crumb = frame::crumb_columns(&s.look, &s.breadcrumb(), 0).unwrap();
        let elsewhere = update(s.clone(), Input::Click { column: 0, row: 0 });
        assert_eq!(elsewhere.mode(), Mode::About);
        let home = update(
            s,
            Input::Click {
                column: crumb.start,
                row: 0,
            },
        );
        assert_eq!((home.mode(), home.cursor), (Mode::Home, 1));
    }

    #[test]
    fn the_wheel_moves_the_home_selection() {
        let s = press(state(80, 24), &[Input::Down, Input::Down, Input::Up]);
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn the_next_input_clears_the_toast_and_a_resize_keeps_it() {
        let s = press(state(120, 40), &[Input::Settings]);
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
    fn settings_toasts_the_milestone_it_arrives_in() {
        let s = press(state(120, 40), &[Input::Settings]);
        assert_eq!(s.toast.as_deref(), Some("Settings: arrives in M3"));
    }

    #[test]
    fn a_number_key_moves_the_cursor_to_its_entry() {
        let s = press(state(120, 40), &[Input::Jump(9)]);
        assert_eq!(ENTRIES[s.cursor].label, "History");
        let s = press(state(120, 40), &[Input::Jump(0)]);
        assert_eq!(ENTRIES[s.cursor].label, "Check a path");
    }

    #[test]
    fn from_an_about_page_zero_and_nine_open_what_works_now() {
        let about = press(state(120, 40), &[Input::Open]);
        assert_eq!(about.mode(), Mode::About);
        assert_eq!(update(about.clone(), Input::Jump(9)).mode(), Mode::History);
        assert_eq!(update(about.clone(), Input::Jump(0)).mode(), Mode::Check);
        assert_eq!(update(about, Input::Jump(4)).mode(), Mode::About);
    }

    #[test]
    fn help_opens_and_closes_on_every_screen_and_swallows_other_keys() {
        for opening in [&[][..], &[Input::Open], &[Input::Jump(9), Input::Open]] {
            let on_screen = press(state(120, 40), opening);
            let screen_mode = on_screen.mode();
            let open = update(on_screen, Input::Help);
            assert_eq!(open.mode(), Mode::Help);
            let still = update(open.clone(), Input::Down);
            assert_eq!(still.mode(), Mode::Help);
            assert_eq!(update(open.clone(), Input::Help).mode(), screen_mode);
            assert_eq!(update(open.clone(), Input::Back).mode(), screen_mode);
            let closed_by_q = update(open, Input::Quit);
            assert_eq!((closed_by_q.mode(), closed_by_q.quit), (screen_mode, None));
        }
    }

    #[test]
    fn q_quits_from_every_screen_but_the_text_field_and_ctrl_c_quits_from_anywhere() {
        for opening in [&[][..], &[Input::Open], &[Input::Jump(9), Input::Open]] {
            let s = press(state(120, 40), opening);
            assert_eq!(update(s, Input::Quit).quit, Some(Quit::Normal));
        }
        let check = press(state(120, 40), &[Input::Jump(0), Input::Open]);
        assert_eq!(check.focus(), Focus::Text);
        assert_eq!(update(check.clone(), Input::QuitNow).quit, Some(Quit::Now));
        let help = press(state(120, 40), &[Input::Help, Input::QuitNow]);
        assert_eq!(help.quit, Some(Quit::Now));
        let small = press(state(50, 12), &[Input::QuitNow]);
        assert_eq!(small.quit, Some(Quit::Now));
    }

    #[test]
    fn in_the_check_field_letters_are_text_and_only_the_field_takes_text() {
        let check = press(state(120, 40), &[Input::Jump(0), Input::Open]);
        let typed = type_text(check.clone(), "q1j");
        let Screen::Check(view) = &typed.screen else {
            panic!("left the Check screen");
        };
        assert_eq!(view.input, "q1j");
        assert_eq!(typed.quit, None);
        assert_eq!(update(typed.clone(), Input::Erase).mode(), Mode::Check);
        let help = update(typed, Input::Help);
        assert_eq!(help.focus(), Focus::Commands, "help closes on ? and q");
        assert_eq!(state(120, 40).focus(), Focus::Commands);
        assert_eq!(state(50, 12).focus(), Focus::Commands);
    }

    #[cfg(unix)]
    #[test]
    fn a_paste_fills_the_field_and_enter_checks_it() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        let mut s = state(120, 40);
        s.facts.env = fixture_env(&dir_path);
        let s = press(s, &[Input::Jump(0), Input::Open]);
        let pasted = format!("{}/My\\ Folder ", dir_path.display());
        let s = press(s, &[Input::Paste(pasted), Input::Open]);
        let Screen::Check(view) = &s.screen else {
            panic!("left the Check screen");
        };
        if !cfg!(windows) {
            assert!(
                view.results[0].path.ends_with("My Folder"),
                "{:?}",
                view.results[0].path
            );
        }
        assert!(view.input.is_empty());
    }

    #[test]
    fn history_scrolls_no_further_than_its_last_row_and_back_to_the_top() {
        let home = tempfile::tempdir().unwrap();
        let mut s = state(60, 16);
        s.facts.env = fixture_env(home.path());
        let s = press(s, &[Input::Jump(9), Input::Open]);
        let Screen::History(view) = &s.screen else {
            panic!("not History");
        };
        assert_eq!(view.scroll, 0);
        let down = press(s.clone(), &vec![Input::Down; 50]);
        let Screen::History(view) = &down.screen else {
            panic!("not History");
        };
        assert!(
            view.scroll <= 1,
            "an empty history has {} rows",
            view.scroll
        );
        let up = press(down, &vec![Input::Up; 50]);
        let Screen::History(view) = &up.screen else {
            panic!("not History");
        };
        assert_eq!(view.scroll, 0);
    }

    #[test]
    fn below_sixty_by_sixteen_waits_for_a_resize_and_the_frame_returns() {
        let small = press(state(50, 12), &[Input::Down, Input::Open, Input::Help]);
        assert_eq!(small.mode(), Mode::TooSmall);
        assert_eq!(
            (small.cursor, small.toast.clone(), small.help),
            (0, None, false)
        );
        assert!(matches!(small.screen, Screen::Home));
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
    fn an_about_page_for_each_planned_entry_names_it() {
        for (index, entry) in ENTRIES.iter().enumerate() {
            if matches!(entry.kind, Kind::Planned { .. }) {
                let s = press(state(80, 24), &vec![Input::Down; index]);
                assert_eq!(entry_with(entry.label), index);
                let s = update(s, Input::Open);
                assert!(matches!(s.screen, Screen::About(i) if i == index));
            }
        }
    }
}
