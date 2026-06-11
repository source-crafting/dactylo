use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Chart, Dataset, GraphType, Paragraph, Tabs};
use ratatui::Frame;

use crate::config::Settings;
use crate::history::{series_of, summary_of, LevelSummary, Record};
use crate::session::SessionResult;

pub enum ResultsAction {
    Restart,
    ChangeSettings,
    Quit,
}

/// The post-session per-level dashboard: level tabs, the selected level's
/// summary, the just-played run, and (later task) the WPM/accuracy charts. All
/// history I/O happens before construction, so this is pure display + nav state
/// and can be re-shown (e.g. returning from setup) without side effects.
pub struct ResultsScreen {
    records: Vec<Record>,
    settings: Settings,
    result: SessionResult,
    cancelled: bool,
    /// Played level's summary BEFORE this run was appended — for the
    /// "this session ▲ vs avg" delta.
    prev_summary: Option<LevelSummary>,
    warning: Option<String>,
    selected_level: u8,
}

impl ResultsScreen {
    pub fn new(
        records: Vec<Record>,
        settings: Settings,
        result: SessionResult,
        cancelled: bool,
        prev_summary: Option<LevelSummary>,
        warning: Option<String>,
    ) -> Self {
        ResultsScreen {
            selected_level: settings.level,
            records,
            settings,
            result,
            cancelled,
            prev_summary,
            warning,
        }
    }

    /// The just-played settings (level + duration), for restart/change-settings.
    pub fn settings(&self) -> Settings {
        self.settings
    }

    #[cfg(test)]
    pub fn selected_level(&self) -> u8 {
        self.selected_level
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Option<ResultsAction> {
        match code {
            KeyCode::Left => {
                self.selected_level = self.selected_level.saturating_sub(1).max(1);
                None
            }
            KeyCode::Right => {
                self.selected_level = (self.selected_level + 1).min(5);
                None
            }
            KeyCode::Enter => Some(ResultsAction::Restart),
            KeyCode::Char('s') | KeyCode::Char('S') => Some(ResultsAction::ChangeSettings),
            KeyCode::Char('q') | KeyCode::Esc => Some(ResultsAction::Quit),
            _ => None,
        }
    }

    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        let title = if self.cancelled {
            format!(" results — {}s · cancelled ", self.settings.duration_secs)
        } else {
            format!(" results — {}s ", self.settings.duration_secs)
        };
        let block = Block::bordered().title(title);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        // The two charts need vertical room; fall back to the text view when the
        // terminal is too small (mirrors the typing screen's size guard).
        if inner.height < 16 || inner.width < 40 {
            self.draw_compact(frame, inner);
        } else {
            self.draw_full(frame, inner);
        }
    }

    /// Level tabs across the top; the played level marked with `*`, the selected
    /// tab highlighted.
    fn tabs(&self) -> Tabs<'static> {
        let titles: Vec<Line> = (1..=5u8)
            .map(|l| {
                let label = if l == self.settings.level {
                    format!("{l}*")
                } else {
                    l.to_string()
                };
                Line::from(format!(" {label} "))
            })
            .collect();
        Tabs::new(titles)
            .select((self.selected_level - 1) as usize)
            .highlight_style(
                Style::new()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )
            .divider(" ")
    }

    /// Text body: per-level summary headline, the this-session line (played tab
    /// only), warning, footer. Also the small-terminal fallback for a later task.
    fn draw_compact(&self, frame: &mut Frame, inner: Rect) {
        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(inner);
        frame.render_widget(self.tabs(), chunks[0]);

        let mut lines: Vec<Line> = Vec::new();
        match summary_of(&self.records, self.selected_level) {
            Some(s) => {
                lines.push(Line::from(format!(
                    "  level {} · {} session(s)",
                    self.selected_level, s.count
                )));
                lines.push(Line::from(format!(
                    "  wpm   avg {:.1}  best {:.1}",
                    s.avg_wpm, s.best_wpm
                )));
                lines.push(Line::from(format!(
                    "  acc   avg {:.1}%  best {:.1}%",
                    s.avg_accuracy, s.best_accuracy
                )));
            }
            None => lines.push(Line::from(format!(
                "  level {} · no sessions yet",
                self.selected_level
            ))),
        }

        if self.selected_level == self.settings.level {
            lines.push(Line::default());
            lines.push(self.this_session_line());
        }

        if let Some(w) = &self.warning {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                format!("  warning: {w}"),
                Style::new().fg(Color::Yellow),
            )));
        }

        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "  enter restart · s settings · ←/→ level · q/esc quit",
            Style::new().fg(Color::DarkGray),
        )));

        frame.render_widget(Paragraph::new(lines), chunks[1]);
    }

    /// Full dashboard: tabs, one headline + this-session line, then stacked WPM
    /// and accuracy charts, then the footer.
    fn draw_full(&self, frame: &mut Frame, inner: Rect) {
        let chunks = Layout::vertical([
            Constraint::Length(1), // tabs
            Constraint::Length(2), // headline + this-session
            Constraint::Min(4),    // wpm chart
            Constraint::Min(4),    // accuracy chart
            Constraint::Length(1), // footer
        ])
        .split(inner);

        frame.render_widget(self.tabs(), chunks[0]);

        let summary = summary_of(&self.records, self.selected_level);
        let mut head: Vec<Line> = Vec::new();
        match &summary {
            Some(s) => head.push(Line::from(format!(
                "  level {} · {} session(s)",
                self.selected_level, s.count
            ))),
            None => head.push(Line::from(format!(
                "  level {} · no sessions yet",
                self.selected_level
            ))),
        }
        if self.selected_level == self.settings.level {
            head.push(self.this_session_line());
        } else if let Some(w) = &self.warning {
            head.push(Line::from(Span::styled(
                format!("  warning: {w}"),
                Style::new().fg(Color::Yellow),
            )));
        }
        frame.render_widget(Paragraph::new(head), chunks[1]);

        let series = series_of(&self.records, self.selected_level);
        let wpm: Vec<(f64, f64)> = series
            .iter()
            .enumerate()
            .map(|(i, p)| ((i + 1) as f64, p.wpm))
            .collect();
        let acc: Vec<(f64, f64)> = series
            .iter()
            .enumerate()
            .map(|(i, p)| ((i + 1) as f64, p.accuracy))
            .collect();

        let wpm_title = match &summary {
            Some(s) => format!(" wpm   avg {:.0}  best {:.0} ", s.avg_wpm, s.best_wpm),
            None => " wpm ".to_string(),
        };
        let acc_title = match &summary {
            Some(s) => format!(
                " acc   avg {:.0}%  best {:.0}% ",
                s.avg_accuracy, s.best_accuracy
            ),
            None => " acc ".to_string(),
        };
        render_metric_chart(frame, chunks[2], &wpm_title, &wpm, Color::Green);
        render_metric_chart(frame, chunks[3], &acc_title, &acc, Color::Cyan);

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  enter restart · s settings · ←/→ level · q/esc quit",
                Style::new().fg(Color::DarkGray),
            ))),
            chunks[4],
        );
    }

    /// "this session  62.0 wpm · 96.2%  ▲ +3.8 vs avg" (or a cancelled variant).
    fn this_session_line(&self) -> Line<'static> {
        let mut spans = vec![Span::raw(format!(
            "  this session  {:.1} wpm · {:.1}%",
            self.result.wpm, self.result.accuracy
        ))];
        if self.cancelled {
            spans.push(Span::styled(
                "  (not saved)",
                Style::new().fg(Color::Yellow),
            ));
        } else if let Some(prev) = &self.prev_summary {
            let delta = self.result.wpm - prev.avg_wpm;
            let (arrow, color) = if delta >= 0.0 {
                ("▲", Color::Green)
            } else {
                ("▼", Color::Red)
            };
            spans.push(Span::styled(
                format!("  {arrow} {delta:+.1} vs avg"),
                Style::new().fg(color),
            ));
        }
        Line::from(spans)
    }
}

/// Render one metric as a braille line chart with its own Y scale. Empty data
/// shows a placeholder. A flat or single-point series gets padded bounds so the
/// axis has non-zero height.
fn render_metric_chart(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    data: &[(f64, f64)],
    color: Color,
) {
    if data.is_empty() {
        frame.render_widget(
            Paragraph::new(format!("{title}— no sessions yet"))
                .style(Style::new().fg(Color::DarkGray)),
            area,
        );
        return;
    }
    let n = data.len();
    let (min_y, max_y) = data.iter().fold((f64::MAX, f64::MIN), |(mn, mx), &(_, y)| {
        (mn.min(y), mx.max(y))
    });
    let (lo, hi) = if (max_y - min_y).abs() < 1e-9 {
        (min_y - 1.0, max_y + 1.0)
    } else {
        (min_y, max_y)
    };
    let dataset = Dataset::default()
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::new().fg(color))
        .data(data);
    let chart = Chart::new(vec![dataset])
        .block(Block::default().title(title.to_string()))
        .x_axis(Axis::default().bounds([1.0, (n.max(2)) as f64]))
        .y_axis(
            Axis::default()
                .bounds([lo, hi])
                .labels(vec![format!("{lo:.0}"), format!("{hi:.0}")]),
        );
    frame.render_widget(chart, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use crate::session::SessionResult;
    use ratatui::crossterm::event::KeyCode;

    fn result() -> SessionResult {
        SessionResult {
            wpm: 50.0,
            raw_wpm: 55.0,
            accuracy: 96.0,
            errors: 3,
            consistency: 90.0,
            chars: 250,
        }
    }

    fn screen(level: u8) -> ResultsScreen {
        ResultsScreen::new(
            Vec::new(),
            Settings {
                duration_secs: 60,
                level,
            },
            result(),
            false,
            None,
            None,
        )
    }

    #[test]
    fn initial_selected_level_is_played_level() {
        assert_eq!(screen(4).selected_level(), 4);
    }

    #[test]
    fn left_right_change_and_clamp_selected_level() {
        let mut s = screen(3);
        assert!(s.handle_key(KeyCode::Right).is_none());
        assert_eq!(s.selected_level(), 4);
        s.handle_key(KeyCode::Right);
        assert_eq!(s.selected_level(), 5);
        s.handle_key(KeyCode::Right); // clamp at 5
        assert_eq!(s.selected_level(), 5);
        for _ in 0..6 {
            s.handle_key(KeyCode::Left);
        }
        assert_eq!(s.selected_level(), 1); // clamp at 1
    }

    #[test]
    fn enter_restarts() {
        assert!(matches!(
            screen(3).handle_key(KeyCode::Enter),
            Some(ResultsAction::Restart)
        ));
    }

    #[test]
    fn s_opens_settings() {
        assert!(matches!(
            screen(3).handle_key(KeyCode::Char('s')),
            Some(ResultsAction::ChangeSettings)
        ));
        assert!(matches!(
            screen(3).handle_key(KeyCode::Char('S')),
            Some(ResultsAction::ChangeSettings)
        ));
    }

    #[test]
    fn q_and_esc_quit() {
        assert!(matches!(
            screen(3).handle_key(KeyCode::Char('q')),
            Some(ResultsAction::Quit)
        ));
        assert!(matches!(
            screen(3).handle_key(KeyCode::Esc),
            Some(ResultsAction::Quit)
        ));
    }

    #[test]
    fn unbound_keys_are_ignored() {
        assert!(screen(3).handle_key(KeyCode::Char('r')).is_none());
    }
}
