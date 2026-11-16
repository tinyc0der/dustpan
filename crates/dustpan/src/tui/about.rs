//! The page behind a planned entry: what it will do, and when it arrives.
use super::frame;
use super::home::{self, ENTRIES, Kind};
use super::rows::{self, Row, Tone};
use super::theme::Look;
use ratatui::Frame;
use ratatui::layout::Rect;

pub fn rows(look: &Look, index: usize, width: usize) -> Vec<Row> {
    let entry = &ENTRIES[index];
    let Kind::Planned { milestone, will_do } = entry.kind else {
        return vec![Row::of(Tone::Plain, format!(" {}", entry.blurb))];
    };
    let text_width = width.saturating_sub(5);
    let mut rows = vec![
        Row::of(Tone::Accent, format!(" {}", entry.label)),
        Row::of(Tone::Plain, format!(" {}", entry.blurb)),
        Row::blank(),
        Row::of(Tone::Bold, " What it will do"),
    ];
    for line in will_do {
        for (i, part) in frame::wrap(line, text_width).into_iter().enumerate() {
            let lead = if i == 0 { look.glyphs.bullet } else { " " };
            rows.push(Row::of(Tone::Plain, format!("  {lead} {part}")));
        }
    }
    rows.push(Row::blank());
    rows.push(Row::of(
        Tone::Plain,
        format!(" Arrives in milestone M{milestone}."),
    ));
    rows.push(Row::blank());
    rows.push(Row::of(Tone::Dim, format!(" {}", until_then())));
    rows
}

fn until_then() -> String {
    let mut ready: Vec<_> = home::ready_entries().collect();
    ready.sort_by_key(|entry| entry.key);
    let tools: Vec<String> = ready
        .iter()
        .map(|entry| format!("{} ({})", entry.label, entry.key))
        .collect();
    format!("Until then: {}", tools.join(" or "))
}

pub fn render(frame: &mut Frame, look: &Look, index: usize, body: Rect) {
    let rows = rows(look, index, body.width as usize);
    rows::render(frame, look, &rows, 0, body);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_names_its_milestone_and_what_works_now() {
        let look = Look::new(false, true);
        let clean: Vec<String> = rows(&look, 0, 80).iter().map(Row::text).collect();
        assert!(clean.contains(&" Clean".to_string()));
        assert!(clean.contains(&" Arrives in milestone M3.".to_string()));
        assert!(
            clean.contains(&" Until then: Check a path (0) or History (9)".to_string()),
            "{clean:?}"
        );
    }

    #[test]
    fn every_planned_page_fits_the_smallest_window_without_scrolling() {
        let look = Look::new(false, true);
        for (index, entry) in ENTRIES.iter().enumerate() {
            if matches!(entry.kind, Kind::Planned { .. }) {
                let height = frame::regions(Rect::new(0, 0, 60, 16)).body.height as usize;
                assert!(
                    rows(&look, index, 60).len() <= height,
                    "{} needs scrolling",
                    entry.label
                );
            }
        }
    }
}
