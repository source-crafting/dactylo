use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Sparkline, Tabs};
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
/// summary, the just-played run, and the WPM/accuracy charts. All
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
        // The sparkline view needs ~8 rows; fall back to the text view when the
        // terminal is too small (mirrors the typing screen's size guard).
        if inner.height < 8 || inner.width < 40 {
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
    /// only), warning, footer. Also the small-terminal fallback when the charts don't fit.
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

    /// Full dashboard: tabs, a session count, then a compact normalized
    /// sparkline + numbers for WPM and for accuracy, then the footer. Each
    /// sparkline shows its metric's variation across the selected level's
    /// sessions; the leading number is the most recent session (this run on the
    /// played level's tab).
    fn draw_full(&self, frame: &mut Frame, inner: Rect) {
        // Tabs at the top, footer at the bottom, the metric block centered
        // between them (equal flexible spacers above and below).
        let chunks = Layout::vertical([
            Constraint::Length(1), // 0 tabs
            Constraint::Min(0),    // 1 top spacer
            Constraint::Length(1), // 2 level · N sessions
            Constraint::Length(1), // 3 wpm numbers + sparkline
            Constraint::Length(1), // 4 acc numbers + sparkline
            Constraint::Length(2), // 5 note (cancelled run + / or warning)
            Constraint::Min(0),    // 6 bottom spacer
            Constraint::Length(1), // 7 footer
        ])
        .split(inner);

        frame.render_widget(self.tabs(), chunks[0]);

        let summary = summary_of(&self.records, self.selected_level);

        let count = match &summary {
            Some(s) => format!("  level {} · {} session(s)", self.selected_level, s.count),
            None => format!("  level {} · no sessions yet", self.selected_level),
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                count,
                Style::new().fg(Color::DarkGray),
            ))),
            chunks[2],
        );

        if let Some(s) = &summary {
            let series = series_of(&self.records, self.selected_level);
            let wpm_vals: Vec<f64> = series.iter().map(|p| p.wpm).collect();
            let acc_vals: Vec<f64> = series.iter().map(|p| p.accuracy).collect();
            let latest_wpm = wpm_vals.last().copied().unwrap_or(0.0);
            let latest_acc = acc_vals.last().copied().unwrap_or(0.0);

            // WPM numbers + delta (played, completed tab only).
            let mut wpm_spans = vec![
                Span::styled(
                    format!("  wpm  {latest_wpm:>4.0}"),
                    Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("  avg {:.0}  best {:.0}", s.avg_wpm, s.best_wpm)),
            ];
            if self.selected_level == self.settings.level && !self.cancelled {
                if let Some(prev) = &self.prev_summary {
                    let delta = self.result.wpm - prev.avg_wpm;
                    let (arrow, color) = if delta >= 0.0 {
                        ("▲", Color::Green)
                    } else {
                        ("▼", Color::Red)
                    };
                    wpm_spans.push(Span::styled(
                        format!("  {arrow} {delta:+.0}"),
                        Style::new().fg(color),
                    ));
                }
            }
            draw_metric_row(
                frame,
                chunks[3],
                Line::from(wpm_spans),
                &wpm_vals,
                Color::Green,
            );

            let acc_line = Line::from(vec![
                Span::styled(
                    format!("  acc  {latest_acc:>3.0}%"),
                    Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(
                    "  avg {:.0}%  best {:.0}%",
                    s.avg_accuracy, s.best_accuracy
                )),
            ]);
            draw_metric_row(frame, chunks[4], acc_line, &acc_vals, Color::Cyan);
        } else {
            // No sessions at this level: explain the empty metric area.
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  play this level to start a trend",
                    Style::new().fg(Color::DarkGray),
                ))),
                chunks[3],
            );
        }

        // Note area: a cancelled run isn't in the series, so surface it here;
        // also show any history warning, on any tab.
        let mut note: Vec<Line> = Vec::new();
        if self.cancelled && self.selected_level == self.settings.level {
            note.push(self.this_session_line());
        }
        if let Some(w) = &self.warning {
            note.push(Line::from(Span::styled(
                format!("  warning: {w}"),
                Style::new().fg(Color::Yellow),
            )));
        }
        if !note.is_empty() {
            frame.render_widget(Paragraph::new(note), chunks[5]);
        }

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  enter restart · s settings · ←/→ level · q/esc quit",
                Style::new().fg(Color::DarkGray),
            ))),
            chunks[7],
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

/// Scale each value into 10..=100 relative to the series' own min/max so the
/// sparkline shows the metric's variation (a flat series renders mid-height).
/// Returns bar heights for ratatui's `Sparkline`, used with `.max(100)`.
fn normalized_bars(values: &[f64]) -> Vec<u64> {
    if values.is_empty() {
        return Vec::new();
    }
    let (min, max) = values
        .iter()
        .fold((f64::MAX, f64::MIN), |(mn, mx), &v| (mn.min(v), mx.max(v)));
    if (max - min).abs() < 1e-9 {
        return vec![50; values.len()];
    }
    values
        .iter()
        .map(|&v| (10.0 + (v - min) / (max - min) * 90.0).round() as u64)
        .collect()
}

/// Render one metric row: a fixed-width numbers column on the left and a
/// one-row sparkline of the series filling the rest of the line.
fn draw_metric_row(frame: &mut Frame, area: Rect, numbers: Line, values: &[f64], color: Color) {
    let cols = Layout::horizontal([Constraint::Length(38), Constraint::Min(0)]).split(area);
    frame.render_widget(Paragraph::new(numbers), cols[0]);
    let bars = normalized_bars(values);
    frame.render_widget(
        Sparkline::default()
            .data(&bars)
            .max(100)
            .style(Style::new().fg(color)),
        cols[1],
    );
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
