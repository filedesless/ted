use crate::ted::buffer::InputMode;
use crate::ted::buffer::Lines;
use crate::ted::Buffer;
use tui::layout::Rect;
use tui::style::Color;
use tui::style::Style;
use tui::text::Span;
use tui::text::Spans;
use tui::widgets::StatefulWidget;

pub struct BufferWidget {}

impl StatefulWidget for BufferWidget {
    type State = Buffer;
    fn render(self, area: Rect, buf: &mut tui::buffer::Buffer, state: &mut Self::State) {
        let (cursor, line_number, column_number) = state.get_cursor();
        let status_line_number = area.height.saturating_sub(1);

        // draw lines from buffer
        let default_style = syntect::highlighting::Style {
            foreground: syntect::highlighting::Color::WHITE,
            background: syntect::highlighting::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0xff,
            },
            font_style: syntect::highlighting::FontStyle::default(),
        };
        let lines = match state.get_visible_lines() {
            Lines::Highlighted(lines) => lines,
            Lines::Plain(lines) => lines
                .iter()
                .cloned()
                .map(|line| {
                    let n = line.len();
                    (line, vec![(default_style, 0..n)])
                })
                .collect(),
        };

        for y in 0..status_line_number {
            if let Some((line, ranges)) = lines.get(y as usize) {
                let window = state.get_window();
                if y == (line_number - window.start) as u16 {
                    if let Some(color) = state
                        .get_highlighter()
                        .as_ref()
                        .and_then(|h| h.theme.settings.line_highlight)
                    {
                        buf.set_style(
                            Rect::new(0, y, area.width, 1),
                            Style::default().bg(Color::Rgb(color.r, color.g, color.b)),
                        )
                    }
                }
                let spans = Spans::from(
                    ranges
                        .iter()
                        .map(|(style, r)| {
                            Span::styled(
                                if state.get_config().show_whitespace {
                                    line[r.clone()].replace("\n", "¶")
                                } else {
                                    line[r.clone()].to_string()
                                },
                                Style::default().fg(Color::Rgb(
                                    style.foreground.r,
                                    style.foreground.g,
                                    style.foreground.b,
                                )),
                            )
                        })
                        .collect::<Vec<Span>>(),
                );
                buf.set_spans(0, y, &spans, area.width);
            } else if state.get_config().show_whitespace {
                buf.set_string(0, y, "~", Style::default());
            }
        }
        // draw status line
        let status = match state.mode {
            InputMode::Normal => "NORMAL MODE",
            InputMode::Insert => "INSERT MODE",
        };
        let window = state.get_window();
        let line = format!(
            "{} - {} - ({}x{}) at {} ({}:{}), lines [{} to {}) ({} - {})",
            state.name,
            status,
            area.width,
            area.height,
            cursor,
            line_number,
            column_number,
            window.start,
            window.end,
            state
                .get_highlighter()
                .as_ref()
                .map(|cached| &cached.syntax.name)
                .unwrap_or(&"Plain Text".to_string()),
            state
                .get_highlighter()
                .as_ref()
                .and_then(|cached| cached.theme.name.as_ref())
                .unwrap_or(&"No Theme".to_string()),
        );
        buf.set_string(0, status_line_number, line, Style::default());
    }
}
