//! The About, History and Check screens drawn into a test terminal, against a fixture home.
use dustpan::tui::app::{Size, State, update, view};
use dustpan::tui::facts::{Environment, Facts};
use dustpan::tui::keys::Input;
use dustpan::tui::theme::Look;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use unicode_width::UnicodeWidthStr;

fn unicode() -> Look {
    Look::new(false, true)
}

fn draw(state: &State) -> Buffer {
    let backend = TestBackend::new(state.size.width, state.size.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| view(state, frame)).unwrap();
    terminal.backend().buffer().clone()
}

fn screen(state: &State) -> String {
    let buffer = draw(state);
    let width = buffer.area.width as usize;
    (0..buffer.area.height as usize)
        .map(|y| {
            let mut row = String::new();
            let mut skip = 0;
            for x in 0..width {
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                let symbol = buffer[(x as u16, y as u16)].symbol();
                skip = symbol.width().saturating_sub(1);
                row.push_str(symbol);
            }
            row.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn after(state: State, inputs: &[Input]) -> State {
    inputs.iter().cloned().fold(state, update)
}

fn type_in(state: State, text: &str) -> State {
    text.chars().fold(state, |s, c| update(s, Input::Text(c)))
}

fn env_for(home: &std::path::Path) -> Environment {
    let home = home.to_string_lossy().into_owned();
    Environment::fixed(&[
        ("HOME", home.clone()),
        ("USERPROFILE", home.clone()),
        ("APPDATA", format!("{home}/AppData/Roaming")),
        ("LOCALAPPDATA", format!("{home}/AppData/Local")),
    ])
}

fn canonical_home() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let root = dir.path().canonicalize().unwrap();
    (dir, root)
}

fn state(look: Look, width: u16, height: u16, env: Environment) -> State {
    State::new(look, Facts { volume: None, env }, Size { width, height })
}

const LOG: &[u8] = b"\n# ========== clean run=20261116094500-42-9f3a session started at 2026-11-16 09:45:00 +0700 ==========\n\
[2026-11-16 09:45:01] [clean run=20261116094500-42-9f3a] REMOVED /home/a/Library/Caches/com.example.app/Cache.db (sink=delete size=1500000)\n\
[2026-11-16 09:45:01] [clean run=20261116094500-42-9f3a] SKIPPED /home/a/Library/Caches/com.example.busy (sink=trash reason=owner-running)\n\
# ========== clean run=20261116094500-42-9f3a session ended at 2026-11-16 09:45:02, 1 items, 1.5 MB ==========\n";

fn home_with_log(log: Option<&[u8]>) -> (tempfile::TempDir, Environment) {
    let (dir, root) = canonical_home();
    let env = env_for(&root);
    if let Some(log) = log {
        let folder =
            dustpan_core::paths::log_dir(dustpan_core::paths::Os::current(), &|name| env.get(name))
                .unwrap();
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("operations.log"), log).unwrap();
    }
    (dir, env)
}

/// The log line names the fixture folder, which differs on every run.
fn masked(screen: String) -> String {
    screen
        .lines()
        .map(|line| {
            if line.starts_with(" Log: ") {
                " Log: <fixture>".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn history(look: Look, width: u16, height: u16, env: Environment) -> State {
    after(
        state(look, width, height, env),
        &[Input::Jump(9), Input::Open],
    )
}

fn check(look: Look, width: u16, height: u16, env: Environment) -> State {
    after(
        state(look, width, height, env),
        &[Input::Jump(0), Input::Open],
    )
}

fn no_env() -> Environment {
    Environment::fixed(&[])
}

#[test]
fn history_with_a_log_at_120_columns() {
    let (_home, env) = home_with_log(Some(LOG));
    insta::assert_snapshot!(masked(screen(&history(unicode(), 120, 36, env))));
}

#[test]
fn history_with_a_log_at_60_columns() {
    let (_home, env) = home_with_log(Some(LOG));
    insta::assert_snapshot!(masked(screen(&history(unicode(), 60, 16, env))));
}

#[test]
fn history_with_no_log_says_nothing_has_been_cleaned() {
    let (home, env) = home_with_log(None);
    let text = screen(&history(unicode(), 80, 24, env));
    assert!(text.contains("Nothing has been cleaned yet."), "{text}");
    assert!(text.contains("Every removal dustpan makes will show up here."));
    assert!(text.contains("dustpan › Home › History"));
    assert_eq!(
        std::fs::read_dir(home.path()).unwrap().count(),
        0,
        "opening History created something in the home"
    );
    insta::assert_snapshot!(masked(text));
}

#[test]
fn about_pages_at_the_smallest_size_and_wide() {
    insta::assert_snapshot!(
        "about_installers_at_60_columns",
        screen(&after(
            state(unicode(), 60, 16, no_env()),
            &[Input::Jump(7), Input::Open]
        ))
    );
    insta::assert_snapshot!(
        "about_clean_at_120_columns",
        screen(&after(state(unicode(), 120, 36, no_env()), &[Input::Open]))
    );
}

#[test]
fn check_starts_with_a_placeholder_and_a_promise_to_only_read() {
    let (_home, env) = home_with_log(None);
    let text = screen(&check(unicode(), 80, 24, env));
    assert!(
        text.contains("Type or paste a path, e.g. ~/Library/Caches"),
        "{text}"
    );
    assert!(text.contains("Nothing is changed or removed"));
    assert!(text.contains("dustpan › Home › Check a path"));
    insta::assert_snapshot!(text);
}

#[cfg(unix)]
#[test]
fn check_shows_typed_text_and_stacks_two_verdicts_newest_first() {
    let (_dir, root) = canonical_home();
    std::fs::create_dir_all(root.join("scratch")).unwrap();
    let env = env_for(&root);
    let s = check(unicode(), 120, 36, env);
    let typing = type_in(s.clone(), "~/qu");
    assert!(
        screen(&typing).contains("Path  ~/qu"),
        "{}",
        screen(&typing)
    );
    let s = type_in(s, "~/scratch");
    let s = update(s, Input::Open);
    let protected = if cfg!(windows) { "C:\\Windows" } else { "/usr" };
    let s = type_in(s, protected);
    let s = update(s, Input::Open);
    let text = screen(&s);
    let kept = text.find("Kept").unwrap();
    let eligible = text.find("Eligible").unwrap();
    assert!(kept < eligible, "newest first: {text}");
    assert!(text.contains("dustpan may clean this when Clean arrives"));
}

#[cfg(unix)]
#[test]
fn check_at_the_smallest_size_keeps_the_field_and_a_result() {
    let (_home, env) = home_with_log(None);
    let protected = if cfg!(windows) { "C:\\Windows" } else { "/usr" };
    let s = check(unicode(), 60, 16, env);
    let s = update(type_in(s, protected), Input::Open);
    let text = screen(&s);
    assert!(text.contains("Path"), "{text}");
    assert!(text.contains("Kept"), "{text}");
    assert!(
        text.lines().last().unwrap().contains("Enter Check"),
        "{text}"
    );
}

#[cfg(unix)]
#[test]
fn a_pasted_escaped_path_with_a_trailing_space_is_checked_as_the_real_path() {
    if cfg!(windows) {
        return;
    }
    let (_dir, root) = canonical_home();
    let env = env_for(&root);
    let s = check(unicode(), 120, 36, env);
    let pasted = format!("{}/My\\ Folder ", root.display());
    let s = update(update(s, Input::Paste(pasted)), Input::Open);
    let text = screen(&s);
    assert!(
        text.contains(&format!("{}/My Folder", root.display())),
        "{text}"
    );
    assert!(text.contains("Eligible"), "{text}");
}

#[test]
fn ascii_mode_draws_only_ascii_and_no_color_draws_no_color_on_every_screen() {
    let (_home, env) = home_with_log(Some(LOG));
    for look in [Look::new(true, true), Look::new(false, false)] {
        for key in [3u8, 9, 0] {
            let s = after(
                state(look, 80, 24, env.clone()),
                &[Input::Jump(key), Input::Open],
            );
            let s = if key == 0 {
                update(type_in(s, "/usr"), Input::Open)
            } else {
                s
            };
            if look.glyphs.hline == "-" {
                assert!(screen(&s).is_ascii(), "{}", screen(&s));
            } else {
                for cell in draw(&s).content() {
                    assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset));
                }
            }
        }
    }
}

#[test]
fn every_screen_has_its_own_footer_and_help() {
    let (_home, env) = home_with_log(None);
    let footer = |s: &State| screen(s).lines().last().unwrap().to_string();
    let home = state(unicode(), 120, 36, env.clone());
    assert!(footer(&home).contains("Enter Open"));
    let about = after(home, &[Input::Open]);
    assert!(footer(&about).contains("Esc Back") && footer(&about).contains("0 9 Try it now"));
    let history = history(unicode(), 120, 36, env.clone());
    assert!(footer(&history).contains("Esc Back") && footer(&history).contains("Scroll"));
    let check = check(unicode(), 120, 36, env);
    assert!(footer(&check).contains("Enter Check") && footer(&check).contains("F1 Help"));
    for (s, title) in [
        (&about, "Help for About"),
        (&history, "Help for History"),
        (&check, "Help for Check a path"),
    ] {
        assert!(
            screen(&update(s.clone(), Input::Help)).contains(title),
            "{title}"
        );
    }
}

#[test]
fn every_screen_fits_its_rows_in_the_window_from_60_to_140_columns() {
    let (_home, env) = home_with_log(Some(LOG));
    for width in (60..=140u16).step_by(10) {
        for opening in [[Input::Jump(7)], [Input::Jump(9)], [Input::Jump(0)]] {
            let s = after(
                state(unicode(), width, 16, env.clone()),
                &[opening[0].clone(), Input::Open],
            );
            for line in screen(&s).lines() {
                assert!(line.width() <= width as usize, "{width}: {line}");
            }
        }
    }
}
