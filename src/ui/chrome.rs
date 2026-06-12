use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Horizontal inset applied to every screen's content, in columns.
const MARGIN: u16 = 2;

/// A dotted rule string of the given width.
fn rule_str(width: u16) -> String {
    "┄".repeat(width as usize)
}

/// Render the shared header — `dactylo` (bold) on the left, `right` right-aligned
/// — plus a dim dotted rule, inside `area`. Returns the body `Rect` below the
/// rule (inset by `MARGIN` and one blank row) for the screen to fill.
pub fn header(frame: &mut Frame, area: Rect, right: Line) -> Rect {
    let x = area.x + MARGIN;
    let w = area.width.saturating_sub(MARGIN * 2);
    let title_row = Rect {
        x,
        y: area.y,
        width: w,
        height: 1,
    };
    let cols = Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(title_row);
    frame.render_widget(
        Paragraph::new(Span::styled(
            "dactylo",
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        cols[0],
    );
    frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), cols[1]);

    frame.render_widget(
        Paragraph::new(Span::styled(rule_str(w), Style::new().fg(Color::DarkGray))),
        Rect {
            x,
            y: area.y + 1,
            width: w,
            height: 1,
        },
    );

    Rect {
        x,
        y: area.y + 3,
        width: w,
        height: area.height.saturating_sub(3),
    }
}

/// A line of boxed key chips: each `key` shown reversed/boxed, followed by its
/// dim `action`.
// consumed by the results screen in a later task
#[allow(dead_code)]
pub fn key_hints(pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (key, action) in pairs {
        spans.push(Span::styled(
            format!(" {key} "),
            Style::new().add_modifier(Modifier::REVERSED),
        ));
        spans.push(Span::styled(
            format!(" {action}   "),
            Style::new().fg(Color::DarkGray),
        ));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_repeats_to_width() {
        assert_eq!(rule_str(5), "┄┄┄┄┄");
        assert_eq!(rule_str(0), "");
    }

    #[test]
    fn key_hints_lists_keys_and_actions() {
        let line = key_hints(&[("Enter", "restart"), ("q", "quit")]);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Enter"));
        assert!(text.contains("restart"));
        assert!(text.contains("q"));
        assert!(text.contains("quit"));
    }
}
