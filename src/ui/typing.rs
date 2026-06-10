use std::time::Instant;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::session::{CharState, Session};

/// Number of text lines visible on the typing screen.
const VISIBLE_LINES: usize = 5;

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
        frame.render_widget(
            Paragraph::new("please enlarge terminal").centered(),
            area,
        );
        return;
    }

    let block = Block::bordered().title(format!(" dactylo — level {level} · {duration_secs}s "));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text_width = inner.width.saturating_sub(4) as usize;
    let line_ranges = layout_lines(session.target(), text_width);
    let cursor_line = line_ranges
        .iter()
        .position(|&(s, e)| session.cursor() >= s && session.cursor() < e)
        .unwrap_or(0);
    // Keep the active line as the second visible line so finished text scrolls away.
    let first = cursor_line.saturating_sub(1);
    let visible = &line_ranges[first..(first + VISIBLE_LINES).min(line_ranges.len())];

    let mut lines: Vec<Line> = vec![Line::default()];
    for &(start, end) in visible {
        let mut spans = vec![Span::raw("  ")];
        for i in start..end {
            let mut style = match session.states()[i] {
                CharState::Correct => Style::new().fg(Color::Green),
                CharState::Wrong => Style::new().fg(Color::White).bg(Color::Red),
                CharState::Untyped => Style::new().fg(Color::DarkGray),
            };
            if i == session.cursor() {
                style = style.add_modifier(Modifier::REVERSED);
            }
            spans.push(Span::styled(session.target()[i].to_string(), style));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);

    let secs_left = session.remaining(now).as_secs();
    let bar = format!(
        "⏱  {secs_left}s left      wpm {:>3.0}   acc {:>3.0}%",
        session.live_wpm(now),
        session.live_accuracy()
    );
    let bar_area = Rect {
        x: inner.x + 2,
        y: inner.y + inner.height - 1,
        width: inner.width.saturating_sub(4),
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(bar).style(Style::new().fg(Color::Cyan)),
        bar_area,
    );
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
