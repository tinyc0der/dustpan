use super::theme::Look;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Bold,
    Dim,
    Accent,
}

/// One line of a scrolling screen: styled runs of text. Screens build their rows from data and a
/// width, so the same rows give both the picture and the number of lines to scroll through.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Row(Vec<(Tone, String)>);

impl Row {
    pub fn blank() -> Self {
        Row::default()
    }

    pub fn of(tone: Tone, text: impl Into<String>) -> Self {
        Row(vec![(tone, text.into())])
    }

    pub fn runs(runs: Vec<(Tone, String)>) -> Self {
        Row(runs)
    }

    pub fn text(&self) -> String {
        self.0.iter().map(|(_, text)| text.as_str()).collect()
    }
}

pub fn render(frame: &mut Frame, look: &Look, rows: &[Row], scroll: usize, area: Rect) {
    let lines: Vec<Line> = rows
        .iter()
        .skip(scroll)
        .take(area.height as usize)
        .map(|row| {
            Line::from(
                row.0
                    .iter()
                    .map(|(tone, text)| {
                        let style = match tone {
                            Tone::Plain => ratatui::style::Style::new(),
                            Tone::Bold => look.bold(),
                            Tone::Dim => look.dim(),
                            Tone::Accent => look.accent(),
                        };
                        Span::styled(text.clone(), style)
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// The furthest a screen scrolls: the last row sits at the bottom of the area.
pub fn max_scroll(rows: usize, height: u16) -> usize {
    rows.saturating_sub(height as usize)
}
