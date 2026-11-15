use super::home::ENTRIES;
use super::keys::{self, HOME};
use super::theme::Look;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

const MAX_WIDTH: u16 = 64;

pub fn lines(look: &Look, inner: usize) -> Vec<Line<'static>> {
    let key_width = HOME
        .iter()
        .map(|doc| keys::expand(doc.keys, &look.glyphs).width())
        .max()
        .unwrap_or(0);
    let mut lines = Vec::new();
    for doc in &HOME {
        let keys = keys::expand(doc.keys, &look.glyphs);
        let pad = " ".repeat(key_width - keys.width());
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(keys, look.bold()),
            Span::raw(format!("{pad}  {}", doc.action)),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(" Jump keys", look.bold()));
    let jumps: Vec<String> = ENTRIES
        .iter()
        .map(|entry| format!("{} {}", entry.key, entry.label))
        .collect();
    let mut row = String::new();
    for jump in jumps {
        if !row.is_empty() && row.width() + 2 + jump.width() > inner.saturating_sub(2) {
            lines.push(Line::raw(format!(" {row}")));
            row.clear();
        }
        if !row.is_empty() {
            row.push_str("  ");
        }
        row.push_str(&jump);
    }
    lines.push(Line::raw(format!(" {row}")));
    lines
}

pub fn render(frame: &mut Frame, look: &Look) {
    let area = frame.area();
    let width = area.width.saturating_sub(4).min(MAX_WIDTH);
    let content = lines(look, width.saturating_sub(2) as usize);
    let height = (content.len() as u16 + 2).min(area.height.saturating_sub(2));
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    let title = format!(" {} Help for Home ", look.glyphs.broom);
    let footer = " ? or Esc closes ";
    let block = Block::new()
        .borders(Borders::ALL)
        .border_set(look.glyphs.border())
        .title(Line::styled(title, look.accent()))
        .title_bottom(Line::styled(footer, look.dim()));
    frame.render_widget(Clear, rect);
    frame.render_widget(Paragraph::new(content).block(block), rect);
}
