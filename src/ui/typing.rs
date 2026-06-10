use std::time::Instant;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
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

    let block = Block::bordered().title(format!(" dactylo — level {level} · {duration_secs}s "));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Fixed-width text column, centered within the frame.
    let text_width = DEFAULT_TEXT_WIDTH.min(inner.width);
    let line_ranges = layout_lines(session.target(), text_width as usize);
    let cursor_line = line_ranges
        .iter()
        .position(|&(s, e)| session.cursor() >= s && session.cursor() < e)
        .unwrap_or(0);
    debug_assert!(
        line_ranges.is_empty()
            || line_ranges
                .iter()
                .any(|&(s, e)| session.cursor() >= s && session.cursor() < e),
        "cursor {} outside all line ranges (target len {})",
        session.cursor(),
        session.target().len()
    );
    // The cursor sits on the first visible line until it leaves line 0, then stays
    // on the center line as finished text scrolls up out of view.
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
                style = style.add_modifier(Modifier::REVERSED);
            }
            spans.push(Span::styled(session.target()[i].to_string(), style));
        }
        lines.push(Line::from(spans));
    }
    // Always render exactly VISIBLE_LINES rows so the block stays vertically stable.
    while lines.len() < VISIBLE_LINES as usize {
        lines.push(Line::default());
    }

    // Vertically center the text rows plus a blank gap and the stats line.
    let content_height = VISIBLE_LINES + 2;
    let col_x = inner.x + inner.width.saturating_sub(text_width) / 2;
    let top = inner.y + inner.height.saturating_sub(content_height) / 2;

    let text_area = Rect {
        x: col_x,
        y: top,
        width: text_width,
        height: VISIBLE_LINES,
    };
    frame.render_widget(Paragraph::new(lines), text_area);

    let secs_left = session.remaining(now).as_secs();
    let bar = format!(
        "⏱  {secs_left}s left      wpm {:>3.0}   acc {:>3.0}%",
        session.live_wpm(now),
        session.live_accuracy()
    );
    let bar_area = Rect {
        x: col_x,
        y: top + VISIBLE_LINES + 1,
        width: text_width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(bar)
            .centered()
            .style(Style::new().fg(Color::Cyan)),
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
