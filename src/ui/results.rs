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

/// Which view the results screen is showing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    /// The just-played session's detailed stats and vs-previous comparison.
    Summary,
    /// The per-level history dashboard (sparklines + level tabs).
    History,
}

/// The post-session screen. Lands on a detailed [`View::Summary`] of the run
/// just played; pressing `h` opens [`View::History`], a per-level dashboard of
/// WPM/accuracy sparklines with level tabs. All history I/O happens before
/// construction, so this is pure display + navigation state and can be re-shown
/// (e.g. returning from setup) without side effects.
pub struct ResultsScreen {
    records: Vec<Record>,
    settings: Settings,
    result: SessionResult,
    cancelled: bool,
    /// Played level's summary BEFORE this run was appended — for the
    /// "vs your N previous sessions" comparison on the summary view.
    prev_summary: Option<LevelSummary>,
    warning: Option<String>,
    view: View,
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
            view: View::Summary,
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

    #[cfg(test)]
    pub fn in_history(&self) -> bool {
        self.view == View::History
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Option<ResultsAction> {
        match self.view {
            View::Summary => match code {
                KeyCode::Enter => Some(ResultsAction::Restart),
                KeyCode::Char('s') | KeyCode::Char('S') => Some(ResultsAction::ChangeSettings),
                KeyCode::Char('h') | KeyCode::Char('H') => {
                    self.view = View::History;
                    None
                }
                KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => Some(ResultsAction::Quit),
                _ => None,
            },
            View::History => match code {
                KeyCode::Left => {
                    self.selected_level = self.selected_level.saturating_sub(1).max(1);
                    None
                }
                KeyCode::Right => {
                    self.selected_level = (self.selected_level + 1).min(5);
                    None
                }
                // Esc / h return to the summary; q still quits the app.
                KeyCode::Esc | KeyCode::Char('h') | KeyCode::Char('H') => {
                    self.view = View::Summary;
                    None
                }
                KeyCode::Char('q') | KeyCode::Char('Q') => Some(ResultsAction::Quit),
                _ => None,
            },
        }
    }

    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        let title = match self.view {
            View::Summary if self.cancelled => format!(
                " results — level {} · {}s · cancelled ",
                self.settings.level, self.settings.duration_secs
            ),
            View::Summary => format!(
                " results — level {} · {}s ",
                self.settings.level, self.settings.duration_secs
            ),
            View::History => " history ".to_string(),
        };
        let block = Block::bordered().title(title);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        match self.view {
            View::Summary => self.draw_summary(frame, inner),
            // The sparkline view needs ~8 rows; fall back to text when smaller.
            View::History if inner.height < 8 || inner.width < 40 => {
                self.draw_history_compact(frame, inner)
            }
            View::History => self.draw_history(frame, inner),
        }
    }

    /// The just-played session: headline stats and the vs-previous comparison.
    fn draw_summary(&self, frame: &mut Frame, inner: Rect) {
        let r = &self.result;
        let mut lines: Vec<Line> = vec![
            Line::default(),
            Line::from(Span::styled(
                format!("  wpm          {:>6.1}", r.wpm),
                Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
            )),
            Line::from(format!("  raw wpm      {:>6.1}", r.raw_wpm)),
            Line::from(format!("  accuracy     {:>5.1}%", r.accuracy)),
            Line::from(format!("  errors       {:>6}", r.errors)),
            Line::from(format!("  consistency  {:>5.1}%", r.consistency)),
            Line::default(),
        ];

        if self.cancelled {
            lines.push(Line::from(Span::styled(
                "  session cancelled — not saved to history",
                Style::new().fg(Color::Yellow),
            )));
            lines.push(Line::default());
        }

        match &self.prev_summary {
            Some(sum) => {
                lines.push(Line::from(format!(
                    "  vs your {} previous level-{} session(s):",
                    sum.count, self.settings.level
                )));
                lines.push(delta_line("wpm", r.wpm, sum.avg_wpm, sum.best_wpm, ""));
                lines.push(delta_line(
                    "acc",
                    r.accuracy,
                    sum.avg_accuracy,
                    sum.best_accuracy,
                    "%",
                ));
            }
            None => lines.push(Line::from("  first session at this level — no history yet")),
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
            "  enter restart · s settings · h history · q/esc quit",
            Style::new().fg(Color::DarkGray),
        )));
        frame.render_widget(Paragraph::new(lines), inner);
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

    /// History dashboard: tabs, a session count, then a normalized sparkline +
    /// numbers for WPM and for accuracy, then the footer. The metric block is
    /// centered between the tabs (top) and footer (bottom).
    fn draw_history(&self, frame: &mut Frame, inner: Rect) {
        let chunks = Layout::vertical([
            Constraint::Length(1), // 0 tabs
            Constraint::Min(0),    // 1 top spacer
            Constraint::Length(1), // 2 level · N sessions
            Constraint::Length(1), // 3 wpm numbers + sparkline
            Constraint::Length(1), // 4 acc numbers + sparkline
            Constraint::Length(1), // 5 warning
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

            let wpm_line = Line::from(vec![
                Span::styled(
                    format!("  wpm  {latest_wpm:>4.0}"),
                    Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("  avg {:.0}  best {:.0}", s.avg_wpm, s.best_wpm)),
            ]);
            draw_metric_row(frame, chunks[3], wpm_line, &wpm_vals, Color::Green);

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
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  play this level to start a trend",
                    Style::new().fg(Color::DarkGray),
                ))),
                chunks[3],
            );
        }

        if let Some(w) = &self.warning {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("  warning: {w}"),
                    Style::new().fg(Color::Yellow),
                ))),
                chunks[5],
            );
        }

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  ←/→ level · esc/h back · q quit",
                Style::new().fg(Color::DarkGray),
            ))),
            chunks[7],
        );
    }

    /// Small-terminal fallback for the history view: text only, no sparklines.
    fn draw_history_compact(&self, frame: &mut Frame, inner: Rect) {
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

        if let Some(w) = &self.warning {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                format!("  warning: {w}"),
                Style::new().fg(Color::Yellow),
            )));
        }

        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "  ←/→ level · esc/h back · q quit",
            Style::new().fg(Color::DarkGray),
        )));
        frame.render_widget(Paragraph::new(lines), chunks[1]);
    }
}

/// A "vs previous" comparison line: `wpm  62.0  ▲ +3.8 vs avg (58.2)  best 71.0`.
fn delta_line(label: &str, value: f64, avg: f64, best: f64, unit: &str) -> Line<'static> {
    let delta = value - avg;
    let (arrow, color) = if delta >= 0.0 {
        ("▲", Color::Green)
    } else {
        ("▼", Color::Red)
    };
    Line::from(vec![
        Span::raw(format!("  {label:<4} {value:>5.1}{unit}  ")),
        Span::styled(
            format!("{arrow} {delta:+.1} vs avg ({avg:.1}{unit})"),
            Style::new().fg(color),
        ),
        Span::raw(format!("    best {best:.1}{unit}")),
    ])
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
    fn starts_on_summary_view() {
        assert!(!screen(3).in_history());
    }

    #[test]
    fn initial_selected_level_is_played_level() {
        assert_eq!(screen(4).selected_level(), 4);
    }

    #[test]
    fn summary_enter_restarts() {
        assert!(matches!(
            screen(3).handle_key(KeyCode::Enter),
            Some(ResultsAction::Restart)
        ));
    }

    #[test]
    fn summary_s_opens_settings() {
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
    fn summary_q_and_esc_quit() {
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
    fn summary_arrows_and_unbound_keys_ignored() {
        let mut s = screen(3);
        assert!(s.handle_key(KeyCode::Left).is_none());
        assert!(s.handle_key(KeyCode::Right).is_none());
        assert!(s.handle_key(KeyCode::Char('x')).is_none());
        assert_eq!(s.selected_level(), 3); // arrows do nothing on the summary
        assert!(!s.in_history());
    }

    #[test]
    fn h_opens_history_and_esc_or_h_returns() {
        let mut s = screen(3);
        assert!(s.handle_key(KeyCode::Char('h')).is_none());
        assert!(s.in_history());
        assert!(s.handle_key(KeyCode::Esc).is_none());
        assert!(!s.in_history());
        // `h` toggles back to history again
        s.handle_key(KeyCode::Char('h'));
        assert!(s.in_history());
        // ...and `h` from history returns to the summary
        s.handle_key(KeyCode::Char('h'));
        assert!(!s.in_history());
    }

    #[test]
    fn history_left_right_change_and_clamp_level() {
        let mut s = screen(3);
        s.handle_key(KeyCode::Char('h')); // enter history
        assert!(s.handle_key(KeyCode::Right).is_none());
        assert_eq!(s.selected_level(), 4);
        s.handle_key(KeyCode::Right);
        s.handle_key(KeyCode::Right); // clamp at 5
        assert_eq!(s.selected_level(), 5);
        for _ in 0..6 {
            s.handle_key(KeyCode::Left);
        }
        assert_eq!(s.selected_level(), 1); // clamp at 1
    }

    #[test]
    fn history_q_quits_but_esc_goes_back() {
        let mut s = screen(3);
        s.handle_key(KeyCode::Char('h'));
        assert!(matches!(
            s.handle_key(KeyCode::Char('q')),
            Some(ResultsAction::Quit)
        ));
        assert!(s.in_history()); // q returns Quit without changing the view
        assert!(s.handle_key(KeyCode::Esc).is_none());
        assert!(!s.in_history());
    }
}
