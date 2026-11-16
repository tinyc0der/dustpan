use super::keys::Hint;
use super::theme::{Glyphs, Look};
use dustpan_sys::BootVolume;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub const MIN_WIDTH: u16 = 60;
pub const MIN_HEIGHT: u16 = 16;
pub const DETAIL_MIN_WIDTH: u16 = 100;
const DETAIL_WIDTH: u16 = 38;

#[derive(Clone, Copy, Debug)]
pub struct Regions {
    pub header: Rect,
    pub top_rule: Rect,
    pub body: Rect,
    pub toast: Rect,
    pub bottom_rule: Rect,
    pub footer: Rect,
}

pub fn regions(area: Rect) -> Regions {
    let [header, top_rule, body, toast, bottom_rule, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    Regions {
        header,
        top_rule,
        body,
        toast,
        bottom_rule,
        footer,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pane {
    #[default]
    List,
    Detail,
}

impl Pane {
    pub fn swapped(self) -> Pane {
        match self {
            Pane::List => Pane::Detail,
            Pane::Detail => Pane::List,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Panes {
    pub list: Option<Rect>,
    pub divider: Option<Rect>,
    pub detail: Option<Rect>,
}

pub fn is_narrow(width: u16) -> bool {
    width < DETAIL_MIN_WIDTH
}

pub fn panes(body: Rect, shown: Pane) -> Panes {
    if is_narrow(body.width) {
        let (list, detail) = match shown {
            Pane::List => (Some(body), None),
            Pane::Detail => (None, Some(body)),
        };
        return Panes {
            list,
            divider: None,
            detail,
        };
    }
    let [list, divider, detail] = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(DETAIL_WIDTH),
    ])
    .areas(body);
    Panes {
        list: Some(list),
        divider: Some(divider),
        detail: Some(detail),
    }
}

pub struct Header<'a> {
    pub breadcrumb: &'a [&'a str],
    pub volume: Option<&'a BootVolume>,
    pub admin: bool,
}

pub fn draw_chrome(
    frame: &mut Frame,
    look: &Look,
    header: &Header,
    toast: Option<&str>,
    hints: &[Hint],
) -> Rect {
    let area = frame.area();
    let regions = regions(area);
    let width = area.width as usize;
    let rule = look.glyphs.hline.repeat(width);

    frame.render_widget(
        Paragraph::new(header_line(look, header, width)),
        regions.header,
    );
    frame.render_widget(Paragraph::new(rule.clone()), regions.top_rule);
    frame.render_widget(Paragraph::new(rule), regions.bottom_rule);
    frame.render_widget(
        Paragraph::new(toast_line(look, toast, width)),
        regions.toast,
    );
    frame.render_widget(
        Paragraph::new(footer_line(look, hints, width)),
        regions.footer,
    );
    regions.body
}

pub fn header_line<'a>(look: &Look, header: &Header, width: usize) -> Line<'a> {
    let glyphs = &look.glyphs;
    let sep = format!(" {} ", glyphs.sep);
    let crumbs = std::iter::once("dustpan")
        .chain(header.breadcrumb.iter().copied())
        .collect::<Vec<_>>()
        .join(&sep);
    let left = format!(" {crumbs}");
    let admin = if header.admin { glyphs.admin } else { "" };
    let right = header
        .volume
        .map(|volume| right_candidates(glyphs, volume))
        .unwrap_or_default()
        .into_iter()
        .find(|candidate| left.width() + admin.width() + candidate.width() + 2 <= width)
        .unwrap_or_default();
    let gap = width.saturating_sub(left.width() + admin.width() + right.width());
    let before = if admin.is_empty() { gap } else { gap / 2 };
    Line::from(vec![
        Span::styled(left, look.bold()),
        Span::raw(" ".repeat(before)),
        Span::styled(admin, look.accent()),
        Span::raw(" ".repeat(gap - before)),
        Span::raw(right),
    ])
}

/// The columns of the breadcrumb at `index` (0 is `Home`), after the leading `dustpan`.
pub fn crumb_columns(
    look: &Look,
    breadcrumb: &[&str],
    index: usize,
) -> Option<std::ops::Range<u16>> {
    let sep = format!(" {} ", look.glyphs.sep).width();
    let mut start = 1 + "dustpan".width() + sep;
    for (i, crumb) in breadcrumb.iter().enumerate() {
        let end = start + crumb.width();
        if i == index {
            return Some(start as u16..end as u16);
        }
        start = end + sep;
    }
    None
}

fn right_candidates(glyphs: &Glyphs, volume: &BootVolume) -> Vec<String> {
    let free = format!("{} free ", format_size(volume.free_bytes));
    let name = cut_middle(&volume.name, 24, glyphs.ellipsis);
    vec![format!("{name}  {free}"), free]
}

pub fn toast_line<'a>(look: &Look, toast: Option<&str>, width: usize) -> Line<'a> {
    match toast {
        Some(text) => Line::styled(
            format!(
                " {}",
                cut_end(text, width.saturating_sub(1), look.glyphs.ellipsis)
            ),
            look.accent(),
        ),
        None => Line::raw(""),
    }
}

pub fn visible_hints<'a>(hints: &'a [Hint], glyphs: &Glyphs, width: usize) -> Vec<&'a Hint> {
    let mut shown: Vec<&Hint> = hints.iter().collect();
    while hints_width(&shown, glyphs) > width {
        let Some(drop) = shown
            .iter()
            .enumerate()
            .max_by_key(|(index, hint)| (hint.priority, *index))
            .map(|(index, _)| index)
        else {
            break;
        };
        shown.remove(drop);
    }
    shown
}

fn hints_width(hints: &[&Hint], glyphs: &Glyphs) -> usize {
    hints
        .iter()
        .map(|hint| 2 + hint.keys(glyphs).width() + 1 + hint.label.width())
        .sum::<usize>()
        .saturating_sub(1)
}

pub fn footer_line<'a>(look: &Look, hints: &[Hint], width: usize) -> Line<'a> {
    let mut spans = vec![Span::raw(" ")];
    for (index, hint) in visible_hints(hints, &look.glyphs, width)
        .into_iter()
        .enumerate()
    {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(hint.keys(&look.glyphs), look.bold()));
        spans.push(Span::raw(format!(" {}", hint.label)));
    }
    Line::from(spans)
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

pub fn cut_middle(text: &str, max: usize, ellipsis: &str) -> String {
    if text.width() <= max {
        return text.to_string();
    }
    let budget = max.saturating_sub(ellipsis.width());
    let head_budget = budget.div_ceil(2);
    let head = take_cells(text.chars(), head_budget);
    let tail: String = take_cells(text.chars().rev(), budget - head.width())
        .chars()
        .rev()
        .collect();
    format!("{head}{ellipsis}{tail}")
}

pub fn pad_end(text: &str, width: usize) -> String {
    format!("{text}{}", " ".repeat(width.saturating_sub(text.width())))
}

pub fn pad_start(text: &str, width: usize) -> String {
    format!("{}{text}", " ".repeat(width.saturating_sub(text.width())))
}

pub fn cut_end(text: &str, max: usize, ellipsis: &str) -> String {
    if text.width() <= max {
        return text.to_string();
    }
    let head = take_cells(text.chars(), max.saturating_sub(ellipsis.width()));
    format!("{head}{ellipsis}")
}

fn take_cells(chars: impl Iterator<Item = char>, budget: usize) -> String {
    let mut used = 0;
    let mut out = String::new();
    for c in chars {
        let cell = c.width().unwrap_or(0);
        if used + cell > budget {
            break;
        }
        used += cell;
        out.push(c);
    }
    out
}

pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let word = cut_end(word, width, "");
        if !line.is_empty() && line.width() + 1 + word.width() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_use_decimal_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(9_800_000_000), "9.8 GB");
        assert_eq!(format_size(88_400_000_000), "88 GB");
        assert_eq!(format_size(2_000_000_000_000), "2.0 TB");
    }

    #[test]
    fn cut_middle_keeps_both_ends_within_the_budget() {
        let cut = cut_middle("Macintosh HD - Data volume", 12, "…");
        assert_eq!(cut.width(), 12);
        assert!(cut.starts_with("Macint") && cut.ends_with("olume"));
        assert_eq!(cut_middle("short", 12, "…"), "short");
    }

    #[test]
    fn cut_counts_wide_glyphs_as_two_cells() {
        assert_eq!(cut_end("🧹🧹🧹🧹", 5, "…").width(), 5);
        assert!(cut_middle("🧹🧹🧹🧹🧹🧹", 7, "…").width() <= 7);
    }

    #[test]
    fn wrap_breaks_on_words_within_the_width() {
        let lines = wrap("Free up disk space and more", 12);
        assert_eq!(lines, ["Free up disk", "space and", "more"]);
    }
}
