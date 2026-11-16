use dustpan::tui::app::{Size, State, update, view};
use dustpan::tui::facts::{Environment, Facts};
use dustpan::tui::frame::Pane;
use dustpan::tui::frame::{self, Header};
use dustpan::tui::home::{ENTRIES, Kind};
use dustpan::tui::keys::{self, Input};
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
        env: Environment::fixed(&[]),
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

/// A wide glyph owns two cells, so the cell after it adds nothing.
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
    inputs.iter().cloned().fold(state, update)
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
    let look = unicode();
    let volume = BootVolume {
        name: "A very long volume name that never fits".to_string(),
        free_bytes: 88_400_000_000,
    };
    let header = Header {
        breadcrumb: &["Home"],
        volume: Some(&volume),
        admin: false,
    };
    let roomy = frame::header_line(&look, &header, 80).to_string();
    assert!(roomy.contains("A very l") && roomy.contains('…'), "{roomy}");
    assert!(roomy.trim_end().ends_with("88 GB free"), "{roomy}");
    let tight = frame::header_line(&look, &header, 50).to_string();
    assert!(!tight.contains("volume") && !tight.contains('…'), "{tight}");
    assert!(tight.trim_end().ends_with("88 GB free"), "{tight}");
    assert!(tight.width() <= 50, "{tight}");
}

#[test]
fn home_lists_every_entry_with_its_blurb_and_a_ready_or_soon_tag() {
    let screen = screen(&state(unicode(), 80, 24));
    for label in ["Clean", "Uninstall", "Optimize", "Analyze", "Status"] {
        assert!(screen.contains(label), "missing {label}");
    }
    assert!(!screen.contains("Coming in"), "{screen}");
    assert!(screen.contains("Welcome. Pick a tool with ↑↓ and Enter"));
    for entry in &ENTRIES {
        let row = screen.lines().find(|l| l.contains(entry.label)).unwrap();
        assert!(row.contains(entry.blurb), "{row}");
        let tag = match entry.kind {
            Kind::Ready(_) => "ready",
            Kind::Planned { .. } => "soon",
        };
        assert!(row.trim_end().ends_with(tag), "{row}");
    }
}

#[test]
fn enter_opens_the_about_page_and_esc_returns_to_the_same_row() {
    let s = after(
        state(unicode(), 80, 24),
        &[Input::Down, Input::Down, Input::Open],
    );
    let page = screen(&s);
    assert!(page.contains("dustpan › Home › Analyze"), "{page}");
    assert!(page.contains("What it will do") && page.contains("Arrives in milestone M1."));
    assert!(page.contains("Until then: Check a path (0) or History (9)"));
    let home = screen(&after(s, &[Input::Back]));
    let marked: Vec<&str> = home.lines().filter(|l| l.contains('➤')).collect();
    assert!(marked.len() == 1 && marked[0].contains("Analyze"), "{home}");
}

#[test]
fn the_cursor_row_shows_an_arrow_not_only_a_style() {
    let s = after(state(unicode(), 80, 24), &vec![Input::Down; 4]);
    let screen = screen(&s);
    let marked: Vec<&str> = screen.lines().filter(|l| l.contains('➤')).collect();
    assert_eq!(marked.len(), 1);
    assert!(marked[0].contains("Status"), "{}", marked[0]);
}

fn expected_home_texts(look: &Look, width: u16, pane: Pane) -> Vec<String> {
    let mut expected = vec![
        "dustpan › Home".to_string(),
        "Macintosh HD  88 GB free".to_string(),
    ];
    let hints = keys::Help::Home.hints(width);
    for hint in frame::visible_hints(&hints, &look.glyphs, width as usize) {
        expected.push(format!("{} {}", hint.keys(&look.glyphs), hint.label));
    }
    if pane == Pane::List || width >= 100 {
        let label_width = ENTRIES.iter().map(|e| e.label.len()).max().unwrap();
        for entry in &ENTRIES {
            expected.push(format!(
                "{}  {:<label_width$}  {}",
                entry.key, entry.label, entry.blurb
            ));
        }
    }
    expected
}

fn assert_nothing_wraps(lines: &[String], expected: &[String]) -> Result<(), String> {
    for text in expected {
        let whole = lines
            .iter()
            .filter(|line| line.contains(text.as_str()))
            .count();
        if whole != 1 {
            return Err(format!("{text:?} appears whole on {whole} lines"));
        }
        let words: Vec<&str> = text.split(' ').collect();
        for skip in 1..words.len() {
            let tail = words[skip..].join(" ");
            let orphan = lines
                .iter()
                .find(|line| line.trim_start().starts_with(&tail) && !line.contains(text.as_str()));
            if let Some(orphan) = orphan {
                return Err(format!(
                    "{orphan:?} starts with the tail {tail:?} of {text:?}"
                ));
            }
        }
    }
    Ok(())
}

fn hard_wrap(lines: &[String], width: usize) -> Vec<String> {
    lines
        .iter()
        .flat_map(|line| {
            let chars: Vec<char> = line.chars().collect();
            chars
                .chunks(width.max(1))
                .map(|chunk| chunk.iter().collect::<String>())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn hints_drop_by_priority_and_nothing_wraps() {
    let footer = |width| {
        let rows = rows(&draw(&state(unicode(), width, 20)));
        rows.last().unwrap().clone()
    };
    let wide = footer(120);
    assert!(wide.contains("Ctrl+C Quit now") && wide.contains("Settings"));
    assert!(!wide.contains("Tab"), "{wide}");
    let narrow = footer(70);
    assert!(!narrow.contains("Ctrl+C"), "{narrow}");
    assert!(
        narrow.contains("? Help") && narrow.contains("q Quit") && narrow.contains("Tab Detail"),
        "{narrow}"
    );
    let smallest = footer(60);
    assert!(
        smallest.contains("? Help") && smallest.contains("q Quit"),
        "{smallest}"
    );
    let look = unicode();
    for width in 60..=140u16 {
        for pane in [Pane::List, Pane::Detail] {
            let mut s = state(look, width, 20);
            s.pane = pane;
            let lines = rows(&draw(&s));
            let expected = expected_home_texts(&look, width, pane);
            assert_nothing_wraps(&lines, &expected).unwrap_or_else(|why| panic!("{width}: {why}"));
            assert!(
                lines.last().unwrap().contains("q Quit"),
                "q Quit must survive at {width}"
            );
        }
    }
}

#[test]
fn the_wrap_check_fails_on_a_wrapped_screen() {
    let look = unicode();
    for width in [60u16, 70, 80, 100, 120] {
        let lines = rows(&draw(&state(look, width, 20)));
        let expected = expected_home_texts(&look, width, Pane::List);
        assert_nothing_wraps(&lines, &expected).unwrap();
        let wrapped = hard_wrap(&lines, width as usize - 9);
        assert!(
            assert_nothing_wraps(&wrapped, &expected).is_err(),
            "a screen wrapped nine cells early passed at {width}"
        );
    }
}

#[test]
fn tab_swaps_between_the_list_and_the_detail_pane_below_100_columns() {
    let list = after(state(unicode(), 80, 24), &[Input::Down, Input::Down]);
    let detail = after(list.clone(), &[Input::SwapPane]);
    let list_text = screen(&list);
    assert!(list_text.contains("Uninstall") && !list_text.contains("Arrives in M1"));
    let text = screen(&detail);
    assert!(
        text.contains("Arrives in M1") && !text.contains("Uninstall"),
        "{text}"
    );
    assert!(screen(&after(detail.clone(), &[Input::SwapPane])).contains("Uninstall"));
    let wide = after(state(unicode(), 120, 40), &[Input::SwapPane]);
    assert_eq!(
        wide.pane,
        Pane::List,
        "Tab does nothing when both panes show"
    );
}

#[test]
fn home_at_80_columns_showing_the_detail_pane() {
    let s = after(state(unicode(), 80, 24), &[Input::Down, Input::SwapPane]);
    insta::assert_snapshot!(screen(&s));
}

#[test]
fn every_help_row_names_every_home_key() {
    let screen = screen(&after(state(unicode(), 80, 24), &[Input::Help]));
    for needle in [
        "↑ ↓ j k",
        "Enter → l",
        "0..9",
        "Tab",
        ",",
        "?",
        "Ctrl+C",
        "Wheel, click",
        "Jump keys",
        "9 History",
        "0 Check a path",
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
    assert!(
        screen(&after(state(unicode(), 120, 40), &[Input::Open]))
            .contains("Arrives in milestone M3")
    );
}
