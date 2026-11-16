use super::frame::{self, Pane};
use super::theme::Look;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    History,
    Check,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Ready(Tool),
    Planned {
        milestone: u8,
        will_do: &'static [&'static str],
    },
}

pub struct Entry {
    pub label: &'static str,
    pub key: u8,
    pub blurb: &'static str,
    pub kind: Kind,
}

impl Entry {
    pub fn status(&self) -> String {
        match self.kind {
            Kind::Ready(_) => "Ready".to_string(),
            Kind::Planned { milestone, .. } => format!("Arrives in M{milestone}"),
        }
    }

    fn tag(&self) -> &'static str {
        match self.kind {
            Kind::Ready(_) => "ready",
            Kind::Planned { .. } => "soon",
        }
    }
}

pub const ENTRIES: [Entry; 10] = [
    Entry {
        label: "Clean",
        key: 1,
        blurb: "Clear caches, logs and leftovers",
        kind: Kind::Planned {
            milestone: 3,
            will_do: &[
                "Finds caches, logs and leftovers you can clear.",
                "Shows a preview. Nothing goes until you confirm.",
                "Skips anything the safety guard protects.",
            ],
        },
    },
    Entry {
        label: "Uninstall",
        key: 2,
        blurb: "Remove an app and its leftovers",
        kind: Kind::Planned {
            milestone: 5,
            will_do: &[
                "Lists your apps and the files each one left behind.",
                "Moves the app to the Trash, then clears leftovers.",
                "Keeps your personal data by default.",
            ],
        },
    },
    Entry {
        label: "Analyze",
        key: 3,
        blurb: "See what is using your disk",
        kind: Kind::Planned {
            milestone: 1,
            will_do: &[
                "Shows your biggest folders and files, with sizes.",
                "Lets you drill into folders to see what is inside.",
                "Moves what you pick to the Trash, after you confirm.",
            ],
        },
    },
    Entry {
        label: "Optimize",
        key: 4,
        blurb: "Refresh caches and services",
        kind: Kind::Planned {
            milestone: 4,
            will_do: &[
                "Runs upkeep tasks, such as rebuilding caches.",
                "Reports one result for each task.",
                "Asks for admin access only for tasks that need it.",
            ],
        },
    },
    Entry {
        label: "Status",
        key: 5,
        blurb: "Watch CPU, memory, disk, battery",
        kind: Kind::Planned {
            milestone: 2,
            will_do: &[
                "Live CPU, memory, disk, network and battery.",
                "A health score with a one-line diagnosis.",
            ],
        },
    },
    Entry {
        label: "Purge",
        key: 6,
        blurb: "Remove old projects' build files",
        kind: Kind::Planned {
            milestone: 6,
            will_do: &[
                "Finds build folders like node_modules in projects.",
                "Skips projects you worked on in the last 7 days.",
            ],
        },
    },
    Entry {
        label: "Installers",
        key: 7,
        blurb: "Find installer files to delete",
        kind: Kind::Planned {
            milestone: 6,
            will_do: &[
                "Finds installer files, such as .dmg and .pkg.",
                "Moves the ones you pick to the Trash.",
            ],
        },
    },
    Entry {
        label: "Slim",
        key: 8,
        blurb: "Trim unused code from apps",
        kind: Kind::Planned {
            milestone: 7,
            will_do: &[
                "Drops code for processors you do not use.",
                "Works on a copy, and checks the signature first.",
            ],
        },
    },
    Entry {
        label: "History",
        key: 9,
        blurb: "Review what past cleanups removed",
        kind: Kind::Ready(Tool::History),
    },
    Entry {
        label: "Check a path",
        key: 0,
        blurb: "Ask if dustpan keeps a path",
        kind: Kind::Ready(Tool::Check),
    },
];

const DIVIDER_AFTER: usize = 5;
const TAG_WIDTH: usize = 5;
const LIST_MAX: usize = 76;

pub fn entry_for_key(key: u8) -> Option<usize> {
    ENTRIES.iter().position(|entry| entry.key == key)
}

pub fn ready_entries() -> impl Iterator<Item = &'static Entry> {
    ENTRIES
        .iter()
        .filter(|entry| matches!(entry.kind, Kind::Ready(_)))
}

pub fn welcome(look: &Look) -> String {
    format!(
        "Welcome. Pick a tool with {}{} and Enter, or click one.",
        look.glyphs.up, look.glyphs.down
    )
}

/// The list gives up its blank row first and its divider next, so all ten entries and the
/// welcome line fit the smallest window.
#[derive(Clone, Copy)]
struct Rows {
    blank: bool,
    divider: bool,
}

impl Rows {
    fn for_height(height: u16) -> Self {
        let entries = ENTRIES.len() as u16;
        Rows {
            divider: height > entries + 1,
            blank: height > entries + 2,
        }
    }

    fn first_entry(self) -> u16 {
        1 + u16::from(self.blank)
    }

    fn visual_row(self, entry: usize) -> u16 {
        let past_divider = self.divider && entry >= DIVIDER_AFTER;
        self.first_entry() + entry as u16 + u16::from(past_divider)
    }
}

pub fn entry_at(list: Rect, column: u16, row: u16) -> Option<usize> {
    let inside = column >= list.x
        && column < list.x + list.width
        && row >= list.y
        && row < list.y + list.height;
    if !inside {
        return None;
    }
    let rows = Rows::for_height(list.height);
    let visual = row - list.y;
    (0..ENTRIES.len()).find(|&entry| rows.visual_row(entry) == visual)
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
    let row_width = width.min(LIST_MAX);
    let rows = Rows::for_height(area.height);
    let label_width = ENTRIES.iter().map(|e| e.label.width()).max().unwrap_or(0);
    let mut lines = vec![Line::styled(
        format!(
            " {}",
            frame::cut_end(
                &welcome(look),
                width.saturating_sub(1),
                look.glyphs.ellipsis
            )
        ),
        look.bold(),
    )];
    if rows.blank {
        lines.push(Line::raw(""));
    }
    for (index, entry) in ENTRIES.iter().enumerate() {
        if rows.divider && index == DIVIDER_AFTER {
            let rule = look.glyphs.hline.repeat(row_width.saturating_sub(4));
            lines.push(Line::styled(format!("  {rule}"), look.dim()));
        }
        let selected = index == cursor;
        let mark = if selected { look.glyphs.cursor } else { " " };
        let label = frame::pad_end(entry.label, label_width);
        let lead = format!(" {mark} {}  ", entry.key);
        let ready = matches!(entry.kind, Kind::Ready(_));
        let room = row_width.saturating_sub(lead.width() + label.width() + 2 + TAG_WIDTH + 2);
        let blurb = frame::cut_end(entry.blurb, room, look.glyphs.ellipsis);
        let gap = " ".repeat(room.saturating_sub(blurb.width()) + 2);
        let tag = frame::pad_start(entry.tag(), TAG_WIDTH);
        let line = if selected {
            Line::styled(
                format!("{lead}{label}  {blurb}{gap}{tag} "),
                look.selected(),
            )
        } else {
            let label_style = if ready { look.accent() } else { look.bold() };
            let tag_style = if ready { look.accent() } else { look.dim() };
            Line::from(vec![
                Span::raw(lead),
                Span::styled(label, label_style),
                Span::raw(format!("  {blurb}{gap}")),
                Span::styled(tag, tag_style),
                Span::raw(" "),
            ])
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
    lines.push(Line::raw(format!(" Status  {}", entry.status())));
    lines.push(Line::raw(format!(" Key     {}", entry.key)));
    lines.push(Line::raw(""));
    let next = match entry.kind {
        Kind::Ready(_) => "Press Enter to open it.",
        Kind::Planned { .. } => "Press Enter to see what it will do.",
    };
    lines.push(Line::styled(format!(" {next}"), look.dim()));
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_matches_the_plan() {
        let milestones: Vec<(&str, Option<u8>)> = ENTRIES
            .iter()
            .map(|e| {
                (
                    e.label,
                    match e.kind {
                        Kind::Planned { milestone, .. } => Some(milestone),
                        Kind::Ready(_) => None,
                    },
                )
            })
            .collect();
        assert_eq!(
            milestones,
            [
                ("Clean", Some(3)),
                ("Uninstall", Some(5)),
                ("Analyze", Some(1)),
                ("Optimize", Some(4)),
                ("Status", Some(2)),
                ("Purge", Some(6)),
                ("Installers", Some(6)),
                ("Slim", Some(7)),
                ("History", None),
                ("Check a path", None),
            ]
        );
    }

    #[test]
    fn jump_keys_are_unique_digits_zero_to_nine() {
        let mut keys: Vec<u8> = ENTRIES.iter().map(|e| e.key).collect();
        keys.sort_unstable();
        assert_eq!(keys, (0..=9).collect::<Vec<u8>>());
    }

    #[test]
    fn every_planned_entry_says_what_it_will_do_in_two_to_four_lines() {
        for entry in &ENTRIES {
            if let Kind::Planned { will_do, .. } = entry.kind {
                assert!((2..=4).contains(&will_do.len()), "{}", entry.label);
            }
        }
    }

    #[test]
    fn a_click_finds_its_entry_and_skips_the_divider_in_a_tall_list() {
        let list = Rect::new(0, 2, 80, 20);
        assert_eq!(entry_at(list, 5, 2), None, "welcome row");
        assert_eq!(entry_at(list, 5, 3), None, "blank row");
        assert_eq!(entry_at(list, 5, 4), Some(0));
        assert_eq!(entry_at(list, 5, 8), Some(4));
        assert_eq!(entry_at(list, 5, 9), None, "divider row");
        assert_eq!(entry_at(list, 5, 10), Some(5));
        assert_eq!(entry_at(list, 5, 14), Some(9));
        assert_eq!(entry_at(list, 5, 15), None);
        assert_eq!(entry_at(list, 80, 4), None, "outside the list");
    }

    #[test]
    fn the_smallest_list_drops_the_blank_and_the_divider_so_all_entries_fit() {
        let list = Rect::new(0, 2, 60, 11);
        assert_eq!(entry_at(list, 5, 3), Some(0));
        assert_eq!(entry_at(list, 5, 12), Some(9));
    }
}
