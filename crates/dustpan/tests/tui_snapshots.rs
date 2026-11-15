use dustpan::tui::app::{Size, State, update, view};
use dustpan::tui::home::Facts;
use dustpan::tui::keys::Input;
use dustpan::tui::theme::Look;
use dustpan_sys::BootVolume;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use unicode_width::UnicodeWidthStr;

fn facts() -> Facts {
    Facts {
        volume: Some(BootVolume {
            name: "Macintosh HD".to_string(),
            free_bytes: 88_400_000_000,
        }),
    }
}

fn state(look: Look, width: u16, height: u16) -> State {
    State::new(look, facts(), Size { width, height })
}

fn unicode() -> Look {
    Look::new(false, true)
}

fn draw(state: &State) -> Buffer {
    let backend = TestBackend::new(state.size.width, state.size.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| view(state, frame)).unwrap();
    terminal.backend().buffer().clone()
}

/// One string per row. A wide glyph owns two cells, so the cell after it adds nothing.
fn rows(buffer: &Buffer) -> Vec<String> {
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
        .collect()
}

fn screen(state: &State) -> String {
    rows(&draw(state)).join("\n")
}

fn after(state: State, inputs: &[Input]) -> State {
    inputs.iter().fold(state, |s, input| update(s, *input))
}

#[test]
fn home_at_60_columns() {
    insta::assert_snapshot!(screen(&state(unicode(), 60, 20)));
}

#[test]
fn home_at_80_columns() {
    insta::assert_snapshot!(screen(&state(unicode(), 80, 24)));
}

#[test]
fn home_at_120_columns_with_the_detail_pane() {
    insta::assert_snapshot!(screen(&state(unicode(), 120, 40)));
}

#[test]
fn help_overlay_over_home() {
    insta::assert_snapshot!(screen(&after(state(unicode(), 120, 40), &[Input::Help])));
}

#[test]
fn help_overlay_at_the_smallest_size() {
    insta::assert_snapshot!(screen(&after(state(unicode(), 60, 16), &[Input::Help])));
}

#[test]
fn home_in_ascii() {
    insta::assert_snapshot!(screen(&state(Look::new(true, true), 120, 40)));
}

#[test]
fn too_small_message() {
    insta::assert_snapshot!(screen(&state(unicode(), 50, 12)));
}

#[test]
fn header_shows_the_volume_and_its_free_space_at_120_columns() {
    let screen = screen(&state(unicode(), 120, 40));
    let header = screen.lines().next().unwrap();
    assert!(header.contains("dustpan › Home"), "{header}");
    assert!(
        header.trim_end().ends_with("Macintosh HD  88 GB free"),
        "{header}"
    );
}

#[test]
fn header_drops_the_volume_name_before_the_free_space() {
    let narrow = Look::new(false, true);
    let screen = screen(&state(narrow, 40 + 20, 20));
    let header = screen.lines().next().unwrap();
    assert!(header.contains("88 GB free"));
    let mut tiny = state(narrow, 60, 16);
    tiny.facts.volume.as_mut().unwrap().name =
        "A very long volume name that never fits".to_string();
    let header = screen_of_header(&tiny);
    assert!(
        header.contains("88 GB free") && header.contains('…'),
        "{header}"
    );
}

fn screen_of_header(state: &State) -> String {
    screen(state).lines().next().unwrap().to_string()
}

#[test]
fn home_lists_every_mole_menu_entry_and_every_later_milestone() {
    let screen = screen(&state(unicode(), 80, 24));
    for label in ["Clean", "Uninstall", "Optimize", "Analyze", "Status"] {
        assert!(screen.contains(label), "missing {label}");
    }
    for (label, milestone) in [
        ("Analyze", 1),
        ("Status", 2),
        ("Clean", 3),
        ("History", 3),
        ("Optimize", 4),
        ("Uninstall", 5),
        ("Purge", 6),
        ("Installers", 6),
        ("Slim", 7),
    ] {
        let row = screen.lines().find(|l| l.contains(label)).unwrap();
        assert!(row.contains(&format!("Coming in M{milestone}")), "{row}");
    }
}

#[test]
fn enter_shows_a_toast_and_the_screen_stays_on_home() {
    let s = after(
        state(unicode(), 80, 24),
        &[Input::Down, Input::Down, Input::Open],
    );
    let screen = screen(&s);
    assert!(screen.contains("Analyze: Coming in M1"));
    assert!(screen.contains("dustpan › Home"));
}

#[test]
fn the_cursor_row_shows_an_arrow_not_only_a_style() {
    let s = after(state(unicode(), 80, 24), &[Input::Down; 4]);
    let screen = screen(&s);
    let marked: Vec<&str> = screen.lines().filter(|l| l.contains('➤')).collect();
    assert_eq!(marked.len(), 1);
    assert!(marked[0].contains("Status"), "{}", marked[0]);
}

#[test]
fn hints_drop_by_priority_and_nothing_wraps() {
    let footer = |width| {
        let rows = rows(&draw(&state(unicode(), width, 20)));
        rows.last().unwrap().clone()
    };
    let wide = footer(120);
    assert!(wide.contains("Ctrl+C Quit now") && wide.contains("Settings"));
    let narrow = footer(70);
    assert!(!narrow.contains("Ctrl+C"), "{narrow}");
    assert!(
        narrow.contains("? Help") && narrow.contains("q Quit"),
        "{narrow}"
    );
    let smallest = footer(60);
    assert!(
        smallest.contains("? Help") && smallest.contains("q Quit"),
        "{smallest}"
    );
    for width in 60..=140u16 {
        let line = footer(width);
        assert!(
            line.width() <= width as usize,
            "footer wraps at {width}: {line}"
        );
        assert!(line.contains("q Quit"), "q Quit must survive at {width}");
    }
}

#[test]
fn every_help_row_names_every_home_key() {
    let screen = screen(&after(state(unicode(), 80, 24), &[Input::Help]));
    for needle in [
        "↑ ↓ j k",
        "Enter → l",
        "1..9",
        ",",
        "?",
        "Ctrl+C",
        "Wheel, click",
        "Jump keys",
        "9 History",
    ] {
        assert!(screen.contains(needle), "help lacks {needle}");
    }
}

fn box_corner(buffer: &Buffer, symbol: &str) -> (u16, u16) {
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            if buffer[(x, y)].symbol() == symbol {
                return (x, y);
            }
        }
    }
    panic!("no {symbol} on screen");
}

/// The S4 capture showed a right border one cell left of its corners. Walk every box row and check
/// that the border cell sits in the column of the corners, with the wide broom in the title row.
fn assert_borders_align(look: Look, width: u16, height: u16) {
    let s = after(state(look, width, height), &[Input::Help]);
    let buffer = draw(&s);
    let g = look.glyphs;
    let (left, top) = box_corner(&buffer, g.top_left);
    let (right, top_right_row) = (0..buffer.area.width)
        .rev()
        .map(|x| (x, top))
        .find(|&(x, y)| buffer[(x, y)].symbol() == g.top_right)
        .unwrap();
    assert_eq!(top, top_right_row);
    let (bottom_left, bottom) = box_corner(&buffer, g.bottom_left);
    assert_eq!(left, bottom_left);
    assert_eq!(buffer[(right, bottom)].symbol(), g.bottom_right);
    for y in top + 1..bottom {
        assert_eq!(buffer[(left, y)].symbol(), g.vline, "left border, row {y}");
        assert_eq!(
            buffer[(right, y)].symbol(),
            g.vline,
            "right border, row {y}"
        );
    }
    let title = rows(&buffer)[top as usize].clone();
    assert!(
        title.contains(g.broom),
        "title row lacks the broom: {title}"
    );
}

#[test]
fn box_borders_align_on_every_row_with_a_wide_glyph_in_the_title() {
    for (width, height) in [(60, 16), (80, 24), (120, 40)] {
        assert_borders_align(Look::new(false, true), width, height);
        assert_borders_align(Look::new(true, true), width, height);
    }
}

#[test]
fn ascii_mode_draws_only_ascii_on_every_screen() {
    let look = Look::new(true, true);
    for (width, height) in [(60, 16), (80, 24), (120, 40), (50, 12)] {
        let home = state(look, width, height);
        let help = after(home.clone(), &[Input::Help]);
        let toast = after(home.clone(), &[Input::Open]);
        for s in [home, help, toast] {
            let text = screen(&s);
            assert!(
                text.is_ascii(),
                "{width}x{height}: {}",
                text.chars().find(|c| !c.is_ascii()).unwrap()
            );
        }
    }
}

#[test]
fn no_color_draws_no_foreground_or_background_color() {
    let look = Look::new(false, false);
    for inputs in [&[][..], &[Input::Help], &[Input::Open]] {
        let buffer = draw(&after(state(look, 120, 40), inputs));
        for cell in buffer.content() {
            assert_eq!(
                (cell.fg, cell.bg),
                (Color::Reset, Color::Reset),
                "{:?}",
                cell.symbol()
            );
        }
    }
}

#[test]
fn color_mode_uses_color_only_with_a_word_beside_it() {
    let buffer = draw(&after(state(unicode(), 120, 40), &[Input::Open]));
    assert!(buffer.content().iter().any(|cell| cell.fg != Color::Reset));
    assert!(screen(&after(state(unicode(), 120, 40), &[Input::Open])).contains("Coming in M3"));
}
