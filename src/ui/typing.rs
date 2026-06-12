use std::time::Instant;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::session::{CharState, Session};

/// Number of text lines visible on the typing screen.
const VISIBLE_LINES: u16 = 3;

/// Default width of the centered text column, in columns.
const DEFAULT_TEXT_WIDTH: u16 = 64;

/// Break `target` into `(start, end)` line ranges of at most `width` chars,
/// preferring to break after a space (the space stays at the end of the line,
/// so every target char is rendered exactly once). Words longer than `width`
/// are hard-broken.
pub fn layout_lines(target: &[char], width: usize) -> Vec<(usize, usize)> {
    let mut lines = Vec::new();
    if width == 0 {
        return lines;
    }
    let mut start = 0;
    while start < target.len() {
        let max_end = (start + width).min(target.len());
        if max_end == target.len() {
            lines.push((start, max_end));
            break;
        }
        let mut break_at = None;
        for i in (start..max_end).rev() {
            if target[i] == ' ' {
                break_at = Some(i + 1);
                break;
            }
        }
        let end = break_at.unwrap_or(max_end);
        lines.push((start, end));
        start = end;
    }
    lines
}

pub fn draw(frame: &mut Frame, session: &Session, now: Instant, level: u8, duration_secs: u64) {
    let area = frame.area();
    if area.width < 40 || area.height < 10 {
        frame.render_widget(Paragraph::new("please enlarge terminal").centered(), area);
        return;
    }

    // Header: level · m:ss remaining · faint live wpm/acc.
    let secs_left = session.remaining(now).as_secs();
    let _ = duration_secs;
    let right = Line::from(vec![
        Span::styled("level ", Style::new().fg(Color::DarkGray)),
        Span::styled(level.to_string(), Style::new().add_modifier(Modifier::BOLD)),
        Span::styled(" · ", Style::new().fg(Color::DarkGray)),
        Span::styled(
            format!("{}:{:02}", secs_left / 60, secs_left % 60),
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "  ·  {:.0} wpm · {:.0}%",
                session.live_wpm(now),
                session.live_accuracy()
            ),
            Style::new().fg(Color::DarkGray),
        ),
    ]);
    let body = crate::ui::chrome::header(frame, area, right);

    // Fixed-width text column, centered within the body (text stays left-aligned
    // inside the column so the cursor doesn't jump as line lengths change).
    let text_width = DEFAULT_TEXT_WIDTH.min(body.width);
    let line_ranges = layout_lines(session.target(), text_width as usize);
    let cursor_line = line_ranges
        .iter()
        .position(|&(s, e)| session.cursor() >= s && session.cursor() < e)
        .unwrap_or(0);
    let first = cursor_line.saturating_sub(1);
    let end = (first + VISIBLE_LINES as usize).min(line_ranges.len());

    let mut lines: Vec<Line> = Vec::with_capacity(VISIBLE_LINES as usize);
    for &(start, stop) in &line_ranges[first..end] {
        let mut spans = Vec::with_capacity(stop - start);
        for i in start..stop {
            let mut style = match session.states()[i] {
                CharState::Correct => Style::new().fg(Color::Green),
                CharState::Wrong => Style::new().fg(Color::White).bg(Color::Red),
                CharState::Untyped => Style::new().fg(Color::DarkGray),
            };
            if i == session.cursor() {
                // Cursor highlight: dark text on an amber box.
                style = Style::new().fg(Color::Black).bg(Color::Yellow);
            }
            spans.push(Span::styled(session.target()[i].to_string(), style));
        }
        lines.push(Line::from(spans));
    }
    while lines.len() < VISIBLE_LINES as usize {
        lines.push(Line::default());
    }

    // Center the text block both ways within the body.
    let top = body.y + body.height.saturating_sub(VISIBLE_LINES) / 2;
    let text_area = Rect {
        x: body.x + body.width.saturating_sub(text_width) / 2,
        y: top,
        width: text_width,
        height: VISIBLE_LINES,
    };
    frame.render_widget(Paragraph::new(lines), text_area);
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn breaks_at_spaces_within_width() {
        let chars: Vec<char> = "the quick brown fox".chars().collect();
        let lines = layout_lines(&chars, 10);
        let rendered: Vec<String> = lines
            .iter()
            .map(|&(s, e)| chars[s..e].iter().collect())
            .collect();
        assert_eq!(rendered, vec!["the quick ", "brown fox"]);
    }

    #[test]
    fn hard_breaks_words_longer_than_width() {
        let chars: Vec<char> = "abcdefghijxyz".chars().collect();
        let lines = layout_lines(&chars, 5);
        assert_eq!(lines, vec![(0, 5), (5, 10), (10, 13)]);
    }

    #[test]
    fn lines_tile_the_target_exactly() {
        let chars: Vec<char> = "one two three four five six".chars().collect();
        let lines = layout_lines(&chars, 8);
        assert_eq!(lines.first().unwrap().0, 0);
        assert_eq!(lines.last().unwrap().1, chars.len());
        for w in lines.windows(2) {
            assert_eq!(w[0].1, w[1].0);
        }
        for &(s, e) in &lines {
            assert!(e - s <= 8);
            assert!(e > s);
        }
    }

    #[test]
    fn empty_target_yields_no_lines() {
        assert!(layout_lines(&[], 10).is_empty());
    }
}
