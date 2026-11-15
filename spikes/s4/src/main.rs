use std::io::Write;
use std::time::Duration;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute, terminal};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{symbols, Frame, Terminal};
use unicode_width::UnicodeWidthStr;

struct Glyphs {
    sep: &'static str,
    admin: &'static str,
    broom: &'static str,
    open: &'static str,
    closed: &'static str,
    cursor: &'static str,
    check: &'static str,
    cross: &'static str,
    full: &'static str,
    dot: &'static str,
    spinner: &'static [&'static str],
    eighths: &'static [&'static str],
    hline: &'static str,
    vline: &'static str,
    border: symbols::border::Set,
}

const UNICODE: Glyphs = Glyphs {
    sep: "›",
    admin: "⚙ admin",
    broom: "🧹",
    open: "▾",
    closed: "▸",
    cursor: "➤",
    check: "✓",
    cross: "✗",
    full: "■",
    dot: "·",
    spinner: &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
    eighths: &[" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"],
    hline: "─",
    vline: "│",
    border: symbols::border::PLAIN,
};

const ASCII: Glyphs = Glyphs {
    sep: ">",
    admin: "* admin",
    broom: "~",
    open: "v",
    closed: ">",
    cursor: ">",
    check: "+",
    cross: "x",
    full: "#",
    dot: "-",
    spinner: &["|", "/", "-", "\\"],
    eighths: &[" ", " ", " ", ":", ":", ":", "#", "#", "#"],
    hline: "-",
    vline: "|",
    border: symbols::border::Set {
        top_left: "+",
        top_right: "+",
        bottom_left: "+",
        bottom_right: "+",
        vertical_left: "|",
        vertical_right: "|",
        horizontal_top: "-",
        horizontal_bottom: "-",
    },
};

fn bar(g: &Glyphs, fraction: f64, width: usize) -> String {
    let eighths = (fraction.clamp(0.0, 1.0) * (width * 8) as f64).round() as usize;
    let mut s = g.eighths[8].repeat(eighths / 8);
    if eighths % 8 > 0 {
        s.push_str(g.eighths[eighths % 8]);
    }
    let used = s.width();
    s + &" ".repeat(width.saturating_sub(used))
}

fn pad(s: &str, width: usize) -> String {
    format!("{s}{}", " ".repeat(width.saturating_sub(s.width())))
}

fn draw(f: &mut Frame, g: &Glyphs, tick: usize) {
    let area = f.area();
    let [header, rule1, body, toast, rule2, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let left = format!(" dustpan {s} Clean {s} Review", s = g.sep);
    let right = "Macintosh HD  88 GB free ";
    let mid = g.admin;
    let gap = (area.width as usize).saturating_sub(left.width() + mid.width() + right.width());
    let header_text = format!("{left}{}{mid}{}{right}", " ".repeat(gap / 2), " ".repeat(gap - gap / 2));
    f.render_widget(Paragraph::new(header_text), header);
    f.render_widget(Paragraph::new(g.hline.repeat(area.width as usize)), rule1);
    f.render_widget(Paragraph::new(g.hline.repeat(area.width as usize)), rule2);

    let [list, divider, detail] =
        Layout::horizontal([Constraint::Length(68), Constraint::Length(1), Constraint::Min(20)]).areas(body);
    f.render_widget(Paragraph::new(vec![Line::from(g.vline); divider.height as usize]), divider);

    let rows: [(&str, &str, &str, &str, f64, &str); 9] = [
        ("", "open", "[■]", "Developer tools", 0.43, "6.1 GB"),
        ("", "", "[x]", "  Xcode DerivedData", 0.23, "3.2 GB  Delete"),
        ("", "", "[x]", "  npm cache", 0.08, "1.1 GB  Run npm"),
        ("", "", "[ ]", "  Gradle caches", 0.13, "1.8 GB  Kept: protected"),
        ("", "open", "[■]", "Browsers", 0.17, "2.4 GB"),
        ("cursor", "", "[x]", "  Chrome cache", 0.10, "1.4 GB  Delete"),
        ("", "", "[-]", "  Safari cache", 0.07, "1.0 GB  Safari running"),
        ("", "closed", "[ ]", "System caches", 0.12, "1.7 GB  Needs admin"),
        ("", "closed", "[ ]", "Old downloads broom", 0.05, "0.7 GB  stale"),
    ];
    let mut lines = vec![Line::from(format!(" Selected 9.8 GB of 14.2 GB {} 1,204 items", g.dot)), Line::from("")];
    for (cur, fold, check, name, frac, size) in rows {
        let cur = if cur == "cursor" { g.cursor } else { " " };
        let fold = match fold {
            "open" => g.open,
            "closed" => g.closed,
            _ => " ",
        };
        let check = check.replace('■', g.full);
        let name = name.replace("broom", g.broom);
        let text = format!(" {} {} {} {} {} {}", pad(cur, 1), pad(fold, 1), check, pad(&name, 22), bar(g, frac, 10), size);
        let style = if cur != " " { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default() };
        lines.push(Line::from(Span::styled(text, style)));
    }
    f.render_widget(Paragraph::new(lines), list);

    let [info, glyphs] = Layout::vertical([Constraint::Length(9), Constraint::Min(8)]).areas(detail);
    let info_lines = vec![
        Line::from(" Chrome cache"),
        Line::from(" ~/Library/Caches/Google/Chrome/Default/Cache"),
        Line::from(""),
        Line::from(" Size     1.4 GB on disk"),
        Line::from(" Age      untouched 12 days"),
        Line::from(" Removal  Deleted for good. Chrome"),
        Line::from("          rebuilds it on next launch."),
        Line::from(format!(" Check    Chrome is not running {}", g.check)),
        Line::from(" Inside   3 protected paths are kept"),
    ];
    f.render_widget(Paragraph::new(info_lines), info);
    draw_glyph_box(f, g, Rect { x: glyphs.x + 1, width: glyphs.width.saturating_sub(2).min(46), ..glyphs }, tick);

    let spin = g.spinner[tick % g.spinner.len()];
    f.render_widget(Paragraph::new(format!(" {spin} Sizing System caches {} {} Moved 3 items to Trash", g.dot, g.check)), toast);
    f.render_widget(
        Paragraph::new(" Space Toggle  a All  ←→ Fold  / Filter  p Protect  Enter Clean 9.8 GB  Esc Back  ? Help"),
        footer,
    );
}

fn draw_glyph_box(f: &mut Frame, g: &Glyphs, area: Rect, tick: usize) {
    let inner_w = area.width.saturating_sub(2) as usize;
    let rows: Vec<(String, String)> = vec![
        ("bars".into(), g.eighths[1..].concat()),
        ("bar 5/8".into(), bar(g, 0.625, 8)),
        ("spinner".into(), g.spinner.concat()),
        ("frame".into(), g.spinner[tick % g.spinner.len()].to_string()),
        ("broom".into(), g.broom.into()),
        ("admin".into(), g.admin.into()),
        ("marks".into(), format!("{} {} {} {} {}", g.cursor, g.open, g.closed, g.check, g.cross)),
    ];
    let lines: Vec<Line> = rows
        .into_iter()
        .map(|(label, sample)| Line::from(format!("{}{}|", pad(&format!(" {label}"), 10), pad(&sample, inner_w.saturating_sub(11)))))
        .collect();
    f.render_widget(Paragraph::new(lines).block(Block::default().borders(Borders::ALL).border_set(g.border).title(" Glyph check ")), area);
}

fn audit(path: &str) -> std::io::Result<()> {
    let samples = [
        "─", "│", "┌", "┐", "└", "┘", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█", "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏",
        "🧹", "⚙", "⚙\u{FE0F}", "➤", "▾", "▸", "✓", "✗", "■", "›", "·", "…", "←", "→", "○", "◎", "⊙",
    ];
    enable_raw_mode()?;
    let mut out = std::io::stdout();
    let mut rows = vec!["glyph\tcodepoints\tunicode_width\tterminal_advance\tmatch".to_string()];
    for s in samples {
        write!(out, "\r\x1b[2K{s}")?;
        out.flush()?;
        let measured = cursor::position().ok().map(|(col, _)| col as usize);
        let cps: Vec<String> = s.chars().map(|c| format!("U+{:04X}", c as u32)).collect();
        rows.push(format!(
            "{s}\t{}\t{}\t{}\t{}",
            cps.join(" "),
            s.width(),
            measured.map(|m| m.to_string()).unwrap_or_else(|| "?".into()),
            measured == Some(s.width())
        ));
    }
    write!(out, "\r\x1b[2K")?;
    disable_raw_mode()?;
    std::fs::write(path, rows.join("\n") + "\n")
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    if let Some(path) = flag("--audit") {
        return audit(&path);
    }
    let glyphs = if args.iter().any(|a| a == "--ascii") { &ASCII } else { &UNICODE };
    let hold: u64 = flag("--hold").and_then(|h| h.parse().ok()).unwrap_or(10);
    let marker = flag("--marker");

    enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen, cursor::Hide)?;
    let mut term = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    let size = terminal::size()?;
    let ticks = hold * 10;
    for tick in 0..ticks {
        term.draw(|f| draw(f, glyphs, if marker.is_some() { 2 } else { tick as usize }))?;
        if tick == 5 {
            if let Some(m) = &marker {
                std::fs::write(m, format!("{}x{}\n", size.0, size.1))?;
            }
        }
        if crossterm::event::poll(Duration::from_millis(100))? {
            if let crossterm::event::Event::Key(k) = crossterm::event::read()? {
                if k.code == crossterm::event::KeyCode::Char('q') {
                    break;
                }
            }
        }
    }
    execute!(std::io::stdout(), LeaveAlternateScreen, cursor::Show)?;
    disable_raw_mode()
}
