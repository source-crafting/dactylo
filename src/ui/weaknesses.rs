use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::mistakes::{sort_weaknesses, Sort, Weakness, WeaknessProfile};

pub enum WeaknessAction {
    /// Return to the results screen.
    Back,
    /// Launch a practice session on these weaknesses.
    Practice,
    /// Quit the app.
    Quit,
}

/// The weakness explorer: ranked weak keys / combos / fumbled words, with a
/// sort toggle, reached with `w` from results. Pure display + navigation; the
/// profile was built (history I/O) before construction.
pub struct WeaknessScreen {
    profile: WeaknessProfile,
    sort: Sort,
    /// Level to practice at (the run the user came from).
    level: u8,
}

impl WeaknessScreen {
    pub fn new(profile: WeaknessProfile, level: u8) -> Self {
        let mut screen = WeaknessScreen {
            profile,
            sort: Sort::Rate,
            level: level.clamp(1, 5),
        };
        // Ensure the initial ordering matches the default sort (Rate).
        sort_weaknesses(&mut screen.profile.keys, Sort::Rate);
        sort_weaknesses(&mut screen.profile.combos, Sort::Rate);
        sort_weaknesses(&mut screen.profile.words, Sort::Rate);
        screen
    }

    pub fn level(&self) -> u8 {
        self.level
    }

    #[cfg(test)]
    pub fn profile(&self) -> &WeaknessProfile {
        &self.profile
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Option<WeaknessAction> {
        match code {
            KeyCode::Enter => Some(WeaknessAction::Practice),
            KeyCode::Left | KeyCode::Right => {
                self.sort = match self.sort {
                    Sort::Rate => Sort::Count,
                    Sort::Count => Sort::Rate,
                };
                sort_weaknesses(&mut self.profile.keys, self.sort);
                sort_weaknesses(&mut self.profile.combos, self.sort);
                sort_weaknesses(&mut self.profile.words, self.sort);
                None
            }
            KeyCode::Esc | KeyCode::Char('w') | KeyCode::Char('W') => Some(WeaknessAction::Back),
            KeyCode::Char('q') | KeyCode::Char('Q') => Some(WeaknessAction::Quit),
            _ => None,
        }
    }

    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        let right = Line::from(Span::styled("weaknesses", Style::new().fg(Color::DarkGray)));
        let body = crate::ui::chrome::header(frame, area, right);

        // A very short/narrow terminal can't fit the columns + footer; the
        // manual footer Rect below would otherwise render outside the buffer.
        if body.width < 52 || body.height < 5 {
            frame.render_widget(
                Paragraph::new("please enlarge terminal").style(Style::new().fg(Color::DarkGray)),
                body,
            );
            return;
        }

        if self.profile.is_empty() {
            frame.render_widget(
                Paragraph::new(
                    "Not enough data yet — type a few sessions and your weak spots show up here.",
                )
                .style(Style::new().fg(Color::DarkGray)),
                body,
            );
            return;
        }

        // Three columns: keys, combos, words. (combos is empty until Phase 2.)
        let cols = Layout::horizontal([
            Constraint::Percentage(30),
            Constraint::Percentage(30),
            Constraint::Percentage(40),
        ])
        .split(Rect {
            height: body.height.saturating_sub(2),
            ..body
        });
        render_column(frame, cols[0], "WEAK KEYS", &self.profile.keys, false);
        render_column(frame, cols[1], "COMBOS", &self.profile.combos, false);
        render_column(frame, cols[2], "FUMBLED WORDS", &self.profile.words, true);

        // Footer: window note + key hints, on the last two rows of the body.
        let footer = Rect {
            x: body.x,
            y: body.y + body.height.saturating_sub(2),
            width: body.width,
            height: 2,
        };
        let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(footer);
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("based on your last {} sessions", self.profile.sessions),
                Style::new().fg(Color::DarkGray),
            )),
            rows[0],
        );
        frame.render_widget(
            Paragraph::new(crate::ui::chrome::key_hints(&[
                ("Enter", "practice these"),
                ("←/→", "sort"),
                ("esc", "back"),
            ])),
            rows[1],
        );
    }
}

/// Render a titled column of ranked weaknesses. `word` widens the label field.
fn render_column(frame: &mut Frame, area: Rect, title: &str, items: &[Weakness], word: bool) {
    let mut lines = vec![Line::from(Span::styled(
        title,
        Style::new()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    ))];
    lines.push(Line::default());
    for w in items.iter().take(8) {
        let rate = format!("{:>3.0}%", w.rate * 100.0);
        let label = if word {
            if w.label.chars().count() > 10 {
                let head: String = w.label.chars().take(9).collect();
                format!("{head}…")
            } else {
                format!("{:<10}", w.label)
            }
        } else {
            format!("{:<4}", w.label)
        };
        lines.push(Line::from(vec![
            Span::styled(label, Style::new().fg(Color::Yellow)),
            Span::styled(rate, Style::new().fg(Color::White)),
            Span::styled(
                format!(" ({}/{})", w.bad, w.total),
                Style::new().fg(Color::DarkGray),
            ),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn weakness(label: &str, total: u32, bad: u32) -> Weakness {
        Weakness {
            label: label.to_string(),
            total,
            bad,
            rate: bad as f64 / total as f64,
        }
    }

    fn populated() -> WeaknessProfile {
        WeaknessProfile {
            keys: vec![weakness("b", 39, 12), weakness("p", 79, 15)],
            combos: Vec::new(),
            words: vec![weakness("because", 8, 5)],
            sessions: 12,
        }
    }

    fn render(screen: &WeaknessScreen) -> String {
        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
        term.draw(|f| screen.draw(f)).unwrap();
        let buf = term.backend().buffer().clone();
        buf.content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn enter_launches_practice() {
        let mut s = WeaknessScreen::new(populated(), 3);
        assert!(matches!(
            s.handle_key(KeyCode::Enter),
            Some(WeaknessAction::Practice)
        ));
    }

    #[test]
    fn esc_and_w_go_back() {
        let mut s = WeaknessScreen::new(populated(), 3);
        assert!(matches!(
            s.handle_key(KeyCode::Esc),
            Some(WeaknessAction::Back)
        ));
        assert!(matches!(
            s.handle_key(KeyCode::Char('w')),
            Some(WeaknessAction::Back)
        ));
    }

    #[test]
    fn arrows_toggle_sort_order() {
        // 'a' has more misses but lower rate than 'b': Rate -> b first; Count -> a first.
        let profile = WeaknessProfile {
            keys: vec![weakness("a", 100, 9), weakness("b", 12, 6)],
            combos: Vec::new(),
            words: Vec::new(),
            sessions: 3,
        };
        let mut s = WeaknessScreen::new(profile, 3);
        assert_eq!(s.profile().keys[0].label, "b"); // default Rate
        s.handle_key(KeyCode::Right);
        assert_eq!(s.profile().keys[0].label, "a"); // Count
    }

    #[test]
    fn renders_headings_and_entries() {
        let text = render(&WeaknessScreen::new(populated(), 3));
        assert!(text.contains("WEAK KEYS"));
        assert!(text.contains("FUMBLED WORDS"));
        assert!(text.contains("because"));
        assert!(text.contains("last 12 sessions"));
    }

    #[test]
    fn renders_empty_state() {
        let text = render(&WeaknessScreen::new(WeaknessProfile::default(), 3));
        assert!(text.contains("Not enough data yet"));
    }

    #[test]
    fn tiny_terminal_does_not_panic() {
        let screen = WeaknessScreen::new(populated(), 3);
        let mut term = Terminal::new(TestBackend::new(20, 4)).unwrap();
        term.draw(|f| screen.draw(f)).unwrap(); // must not panic
        let buf = term.backend().buffer().clone();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains("enlarge"));
    }

    #[test]
    fn renders_combos_column_entries() {
        let mut p = populated();
        p.combos = vec![weakness("io", 20, 6)];
        let text = render(&WeaknessScreen::new(p, 3));
        assert!(text.contains("COMBOS"));
        assert!(text.contains("io"));
    }

    #[test]
    fn long_fumbled_word_is_truncated() {
        let profile = WeaknessProfile {
            keys: Vec::new(),
            combos: Vec::new(),
            words: vec![weakness("international", 8, 5)],
            sessions: 4,
        };
        let text = render(&WeaknessScreen::new(profile, 3));
        // "international" is 13 chars; [..9] = "internati", then "…" gives 10 display cols.
        assert!(text.contains("internati…"));
        assert!(!text.contains("international"));
    }
}
