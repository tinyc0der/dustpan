use super::frame::{self, Pane};
use super::theme::Look;
use dustpan_sys::BootVolume;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, Default)]
pub struct Facts {
    pub volume: Option<BootVolume>,
}

pub struct Entry {
    pub label: &'static str,
    pub key: u8,
    pub milestone: u8,
    pub blurb: &'static str,
}

impl Entry {
    pub fn coming_in(&self) -> String {
        format!("Coming in M{}", self.milestone)
    }
}

pub const ENTRIES: [Entry; 9] = [
    Entry {
        label: "Clean",
        key: 1,
        milestone: 3,
        blurb: "Free up disk space",
    },
    Entry {
        label: "Uninstall",
        key: 2,
        milestone: 5,
        blurb: "Remove apps completely",
    },
    Entry {
        label: "Analyze",
        key: 3,
        milestone: 1,
        blurb: "Explore disk usage",
    },
    Entry {
        label: "Optimize",
        key: 4,
        milestone: 4,
        blurb: "Refresh caches and services",
    },
    Entry {
        label: "Status",
        key: 5,
        milestone: 2,
        blurb: "Monitor system health",
    },
    Entry {
        label: "Purge",
        key: 6,
        milestone: 6,
        blurb: "Remove build artifacts from old projects",
    },
    Entry {
        label: "Installers",
        key: 7,
        milestone: 6,
        blurb: "Remove installer files you no longer need",
    },
    Entry {
        label: "Slim",
        key: 8,
        milestone: 7,
        blurb: "Trim app bundles you do not use",
    },
    Entry {
        label: "History",
        key: 9,
        milestone: 3,
        blurb: "Review what past sessions removed",
    },
];

const DIVIDER_AFTER: usize = 5;

pub fn entry_for_key(key: u8) -> Option<usize> {
    ENTRIES.iter().position(|entry| entry.key == key)
}

fn visual_row(entry: usize) -> u16 {
    (entry + usize::from(entry >= DIVIDER_AFTER)) as u16
}

const LIST_TOP_PADDING: u16 = 1;

pub fn entry_at(list: Rect, column: u16, row: u16) -> Option<usize> {
    let inside = column >= list.x
        && column < list.x + list.width
        && row >= list.y + LIST_TOP_PADDING
        && row < list.y + list.height;
    if !inside {
        return None;
    }
    let visual = row - list.y - LIST_TOP_PADDING;
    (0..ENTRIES.len()).find(|&entry| visual_row(entry) == visual)
}

pub fn render(frame: &mut Frame, look: &Look, cursor: usize, shown: Pane, body: Rect) {
    let panes = frame::panes(body, shown);
    if let Some(list) = panes.list {
        render_list(frame, look, cursor, list);
    }
    if let Some(divider) = panes.divider {
        let rule = vec![Line::raw(look.glyphs.vline); divider.height as usize];
        frame.render_widget(Paragraph::new(rule), divider);
    }
    if let Some(detail) = panes.detail {
        render_detail(frame, look, cursor, detail);
    }
}

fn render_list(frame: &mut Frame, look: &Look, cursor: usize, area: Rect) {
    let width = area.width as usize;
    let label_width = ENTRIES.iter().map(|e| e.label.width()).max().unwrap_or(0);
    let mut lines = vec![Line::raw("")];
    for (index, entry) in ENTRIES.iter().enumerate() {
        if index == DIVIDER_AFTER {
            let rule = look.glyphs.hline.repeat(width.saturating_sub(4));
            lines.push(Line::styled(format!("  {rule}"), look.dim()));
        }
        let selected = index == cursor;
        let mark = if selected { look.glyphs.cursor } else { " " };
        let label = format!("{:<label_width$}", entry.label);
        let text = format!(" {mark} {}  {label}  ", entry.key);
        let rest = width.saturating_sub(text.width());
        let status = frame::cut_end(&entry.coming_in(), rest, look.glyphs.ellipsis);
        let padding = " ".repeat(rest.saturating_sub(status.width()));
        let line = if selected {
            Line::styled(format!("{text}{status}{padding}"), look.selected())
        } else {
            Line::from(vec![Span::raw(text), Span::styled(status, look.dim())])
        };
        lines.push(line);
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_detail(frame: &mut Frame, look: &Look, cursor: usize, area: Rect) {
    let entry = &ENTRIES[cursor];
    let inner = area.width.saturating_sub(2) as usize;
    let mut lines = vec![
        Line::raw(""),
        Line::styled(format!(" {}", entry.label), look.accent()),
        Line::raw(""),
    ];
    lines.extend(
        frame::wrap(entry.blurb, inner)
            .into_iter()
            .map(|line| Line::raw(format!(" {line}"))),
    );
    lines.push(Line::raw(""));
    lines.push(Line::raw(format!(" Status  {}", entry.coming_in())));
    lines.push(Line::raw(format!(" Key     {}", entry.key)));
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_matches_the_plan() {
        let milestones: Vec<(&str, u8)> = ENTRIES.iter().map(|e| (e.label, e.milestone)).collect();
        assert_eq!(
            milestones,
            [
                ("Clean", 3),
                ("Uninstall", 5),
                ("Analyze", 1),
                ("Optimize", 4),
                ("Status", 2),
                ("Purge", 6),
                ("Installers", 6),
                ("Slim", 7),
                ("History", 3),
            ]
        );
    }

    #[test]
    fn jump_keys_are_unique_digits_one_to_nine() {
        let mut keys: Vec<u8> = ENTRIES.iter().map(|e| e.key).collect();
        keys.sort_unstable();
        assert_eq!(keys, (1..=9).collect::<Vec<u8>>());
    }

    #[test]
    fn a_click_finds_its_entry_and_skips_the_divider() {
        let list = Rect::new(0, 2, 80, 20);
        assert_eq!(entry_at(list, 5, 2), None, "padding row");
        assert_eq!(entry_at(list, 5, 3), Some(0));
        assert_eq!(entry_at(list, 5, 7), Some(4));
        assert_eq!(entry_at(list, 5, 8), None, "divider row");
        assert_eq!(entry_at(list, 5, 9), Some(5));
        assert_eq!(entry_at(list, 5, 12), Some(8));
        assert_eq!(entry_at(list, 5, 13), None);
        assert_eq!(entry_at(list, 80, 3), None, "outside the list");
    }
}
