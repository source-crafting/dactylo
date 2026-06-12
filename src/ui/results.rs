use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Sparkline, Tabs};
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
            // Clamp defensively: settings.level is always 1..=5 via the validated
            // entry paths, but `tabs()` does `selected_level - 1`, so guard the
            // widget against ever underflowing/over-selecting.
            selected_level: settings.level.clamp(1, 5),
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
        match self.view {
            View::Summary => self.draw_summary(frame, area),
            View::History if area.width < 40 || area.height < 12 => {
                self.draw_history_compact(frame, area)
            }
            View::History => self.draw_history(frame, area),
        }
    }

    /// The just-played session: a 2×3 grid comparing each metric vs your best.
    fn draw_summary(&self, frame: &mut Frame, area: Rect) {
        let dur = self.settings.duration_secs;
        let right = Line::from(vec![
            Span::styled("level ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                self.settings.level.to_string(),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::styled(" · ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                format!("{}:{:02}", dur / 60, dur % 60),
                Style::new().add_modifier(Modifier::BOLD),
            ),
        ]);
        let body = crate::ui::chrome::header(frame, area, right);

        if body.width < 36 || body.height < 9 {
            frame.render_widget(Paragraph::new("please enlarge terminal"), body);
            return;
        }

        let r = &self.result;
        let prev = self.prev_summary.as_ref();

        let wpm = metric_cell(
            "WPM",
            format!("{:.0}", r.wpm),
            "net",
            self.delta(r.wpm, prev.map(|p| p.best_wpm), true),
        );
        let raw = metric_cell(
            "RAW",
            format!("{:.0}", r.raw_wpm),
            "",
            self.delta(r.raw_wpm, prev.map(|p| p.best_raw_wpm), true),
        );
        let acc = metric_cell(
            "ACC",
            format!("{:.0}", r.accuracy),
            "%",
            self.delta(r.accuracy, prev.map(|p| p.best_accuracy), true),
        );
        let errs = metric_cell(
            "ERRORS",
            format!("{}", r.errors),
            "",
            self.delta(r.errors as f64, prev.map(|p| p.min_errors as f64), false),
        );
        let cons = metric_cell(
            "CONSIST",
            format!("{:.0}", r.consistency),
            "",
            self.delta(r.consistency, prev.map(|p| p.best_consistency), true),
        );
        let lvl_delta = match prev {
            Some(p) => dim(format!("your avg {:.0}", p.avg_wpm)),
            None => dim("your avg —"),
        };
        let lvl = metric_cell("LEVEL", self.settings.level.to_string(), "", lvl_delta);

        let rows = Layout::vertical([
            Constraint::Length(3), // row 1 cells
            Constraint::Length(1), // gap
            Constraint::Length(3), // row 2 cells
            Constraint::Min(0),    // spacer
            Constraint::Length(1), // rule
            Constraint::Length(1), // meta
            Constraint::Length(1), // chips
            Constraint::Length(1), // status
        ])
        .split(body);

        render_cells(frame, rows[0], [wpm, raw, acc]);
        render_cells(frame, rows[2], [errs, cons, lvl]);

        frame.render_widget(
            Paragraph::new(dim(crate::ui::chrome::rule_str(body.width))),
            rows[4],
        );

        let mut meta = vec![Span::styled(
            format!("level {} · {dur}s", self.settings.level),
            Style::new().fg(Color::DarkGray),
        )];
        match &self.warning {
            Some(w) => {
                meta.push(Span::styled(" · ", Style::new().fg(Color::DarkGray)));
                meta.push(Span::styled(
                    format!("warning: {w}"),
                    Style::new().fg(Color::Yellow),
                ));
            }
            // Only claim "saved" for a completed run that actually recorded.
            None if !self.cancelled => meta.push(Span::styled(
                " · saved to ~/.dactylo",
                Style::new().fg(Color::DarkGray),
            )),
            None => {} // cancelled, no warning: the status line states it wasn't saved
        }
        frame.render_widget(Paragraph::new(Line::from(meta)), rows[5]);
        frame.render_widget(
            Paragraph::new(crate::ui::chrome::key_hints(&[
                ("Enter", "restart"),
                ("s", "settings"),
                ("h", "history"),
                ("q", "quit"),
            ])),
            rows[6],
        );
        let status = if self.cancelled {
            "session cancelled — not saved"
        } else {
            "time's up"
        };
        frame.render_widget(Paragraph::new(dim(status)), rows[7]);
    }

    /// Build a delta line for a metric, or a dim `—` when there's no prior best.
    fn delta(&self, value: f64, best: Option<f64>, higher_is_better: bool) -> Line<'static> {
        match best {
            Some(best) => {
                let (arrow, text, color) = metric_delta(value, best, higher_is_better);
                Line::from(Span::styled(
                    format!("{arrow} {text}"),
                    Style::new().fg(color),
                ))
            }
            None => dim("—"),
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

    /// History dashboard: tabs, a session count, then a normalized sparkline +
    /// numbers for WPM and for accuracy, then the footer. The metric block is
    /// centered between the tabs (top) and footer (bottom).
    fn draw_history(&self, frame: &mut Frame, area: Rect) {
        let body = crate::ui::chrome::header(
            frame,
            area,
            Line::from(Span::styled(
                "history",
                Style::new().add_modifier(Modifier::BOLD),
            )),
        );
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
        .split(body);

        frame.render_widget(self.tabs(), chunks[0]);

        let summary = summary_of(&self.records, self.selected_level);
        frame.render_widget(
            Paragraph::new(dim(level_count_text(self.selected_level, summary.as_ref()))),
            chunks[2],
        );

        if let Some(s) = &summary {
            let series = series_of(&self.records, self.selected_level);
            let wpm_vals: Vec<f64> = series.iter().map(|p| p.wpm).collect();
            let acc_vals: Vec<f64> = series.iter().map(|p| p.accuracy).collect();
            let latest_wpm = wpm_vals.last().copied().unwrap_or(0.0);
            let latest_acc = acc_vals.last().copied().unwrap_or(0.0);

            let wpm_line =
                metric_numbers("wpm", latest_wpm, s.avg_wpm, s.best_wpm, "", Color::Green);
            draw_metric_row(
                frame,
                chunks[3],
                wpm_line,
                &wpm_vals,
                Color::Green,
                WPM_MIN_SPAN,
            );

            let acc_line = metric_numbers(
                "acc",
                latest_acc,
                s.avg_accuracy,
                s.best_accuracy,
                "%",
                Color::Cyan,
            );
            draw_metric_row(
                frame,
                chunks[4],
                acc_line,
                &acc_vals,
                Color::Cyan,
                ACC_MIN_SPAN,
            );
        } else {
            frame.render_widget(
                Paragraph::new(dim("  play this level to start a trend")),
                chunks[3],
            );
        }

        if let Some(w) = &self.warning {
            frame.render_widget(Paragraph::new(warning_line(w)), chunks[5]);
        }

        frame.render_widget(Paragraph::new(dim(HISTORY_FOOTER)), chunks[7]);
    }

    /// Small-terminal fallback for the history view: same numbers as the full
    /// view but without the sparklines.
    fn draw_history_compact(&self, frame: &mut Frame, area: Rect) {
        let body = crate::ui::chrome::header(
            frame,
            area,
            Line::from(Span::styled(
                "history",
                Style::new().add_modifier(Modifier::BOLD),
            )),
        );
        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(body);
        frame.render_widget(self.tabs(), chunks[0]);

        let summary = summary_of(&self.records, self.selected_level);
        let mut lines: Vec<Line> =
            vec![dim(level_count_text(self.selected_level, summary.as_ref()))];
        if let Some(s) = &summary {
            let series = series_of(&self.records, self.selected_level);
            let latest_wpm = series.last().map(|p| p.wpm).unwrap_or(0.0);
            let latest_acc = series.last().map(|p| p.accuracy).unwrap_or(0.0);
            lines.push(metric_numbers(
                "wpm",
                latest_wpm,
                s.avg_wpm,
                s.best_wpm,
                "",
                Color::Green,
            ));
            lines.push(metric_numbers(
                "acc",
                latest_acc,
                s.avg_accuracy,
                s.best_accuracy,
                "%",
                Color::Cyan,
            ));
        }

        if let Some(w) = &self.warning {
            lines.push(Line::default());
            lines.push(warning_line(w));
        }

        lines.push(Line::default());
        lines.push(dim(HISTORY_FOOTER));
        frame.render_widget(Paragraph::new(lines), chunks[1]);
    }
}

/// A 3-line metric cell: dim label, bold value + dim unit, then the delta line.
fn metric_cell(label: &str, value: String, unit: &str, delta: Line<'static>) -> Paragraph<'static> {
    let value_line = if unit.is_empty() {
        Line::from(Span::styled(
            value,
            Style::new().add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(vec![
            Span::styled(value, Style::new().add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {unit}"), Style::new().fg(Color::DarkGray)),
        ])
    };
    Paragraph::new(vec![dim(label), value_line, delta])
}

/// Render three cells side by side across `area` (each ~a third of the width).
fn render_cells(frame: &mut Frame, area: Rect, cells: [Paragraph<'static>; 3]) {
    let cols = Layout::horizontal([
        Constraint::Percentage(34),
        Constraint::Percentage(33),
        Constraint::Percentage(33),
    ])
    .split(area);
    for (cell, col) in cells.into_iter().zip(cols.iter()) {
        frame.render_widget(cell, *col);
    }
}

/// Compare `value` to your prior `best` for a metric. An improvement (>= best,
/// or <= best for `!higher_is_better`) is a green `▲ best yet`; otherwise a red
/// `▼ {gap} off best`.
fn metric_delta(value: f64, best: f64, higher_is_better: bool) -> (&'static str, String, Color) {
    let improved = if higher_is_better {
        value >= best
    } else {
        value <= best
    };
    if improved {
        ("▲", "best yet".to_string(), Color::Green)
    } else if higher_is_better {
        ("▼", format!("-{:.0} off best", best - value), Color::Red)
    } else {
        ("▼", format!("+{:.0} off best", value - best), Color::Red)
    }
}

/// The footer hint shown on both the full and compact history views.
const HISTORY_FOOTER: &str = "  ←/→ level · esc/h back · q quit";

/// Smallest value range (WPM points / accuracy %) a sparkline spreads to full
/// height; narrower ranges render near-flat so noise isn't exaggerated.
const WPM_MIN_SPAN: f64 = 5.0;
const ACC_MIN_SPAN: f64 = 3.0;

/// A dim (DarkGray) single-line paragraph — used for the count line and footers.
fn dim(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), Style::new().fg(Color::DarkGray)))
}

/// The yellow `warning: …` line shown on any view when history I/O had an issue.
fn warning_line(w: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  warning: {w}"),
        Style::new().fg(Color::Yellow),
    ))
}

/// `  level N · M session(s)`, or `… · no sessions yet` when there are none.
fn level_count_text(level: u8, summary: Option<&LevelSummary>) -> String {
    match summary {
        Some(s) => format!("  level {level} · {} session(s)", s.count),
        None => format!("  level {level} · no sessions yet"),
    }
}

/// One history metric's numbers line: a bold coloured latest value followed by
/// dim avg/best. `unit` is `""` for WPM or `"%"` for accuracy.
fn metric_numbers(
    label: &str,
    latest: f64,
    avg: f64,
    best: f64,
    unit: &str,
    color: Color,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {label}  {latest:>4.0}{unit}"),
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("  avg {avg:.0}{unit}  best {best:.0}{unit}")),
    ])
}

/// Scale each value to a bar height in 10..=100 relative to the series' own
/// range, but never amplifying a range narrower than `min_span` — so trivial
/// session-to-session noise renders near-flat instead of as a full-height spike.
/// The data band is centered within the span. Used with `Sparkline::max(100)`.
/// `min_span` must be > 0. Empty input → empty.
fn normalized_bars(values: &[f64], min_span: f64) -> Vec<u64> {
    if values.is_empty() {
        return Vec::new();
    }
    let (min, max) = values
        .iter()
        .fold((f64::MAX, f64::MIN), |(mn, mx), &v| (mn.min(v), mx.max(v)));
    let actual = max - min;
    let span = actual.max(min_span); // >= min_span > 0, so never divides by zero
    let lo = min - (span - actual) / 2.0; // center the data within the span
    values
        .iter()
        .map(|&v| (10.0 + (v - lo) / span * 90.0).round() as u64)
        .collect()
}

/// Render one metric row: a fixed-width numbers column on the left and a
/// one-row sparkline of the series filling the rest of the line. `min_span` is
/// the smallest value range the sparkline will spread to full height.
fn draw_metric_row(
    frame: &mut Frame,
    area: Rect,
    numbers: Line,
    values: &[f64],
    color: Color,
    min_span: f64,
) {
    let cols = Layout::horizontal([Constraint::Length(38), Constraint::Min(0)]).split(area);
    frame.render_widget(Paragraph::new(numbers), cols[0]);
    let bars = normalized_bars(values, min_span);
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

    #[test]
    fn metric_delta_higher_is_better() {
        let (arrow, text, color) = metric_delta(72.0, 68.0, true);
        assert_eq!(arrow, "▲");
        assert_eq!(text, "best yet");
        assert_eq!(color, ratatui::style::Color::Green);
        let (arrow, text, color) = metric_delta(60.0, 68.0, true);
        assert_eq!(arrow, "▼");
        assert_eq!(text, "-8 off best");
        assert_eq!(color, ratatui::style::Color::Red);
    }

    #[test]
    fn metric_delta_lower_is_better_for_errors() {
        let (arrow, text, _) = metric_delta(6.0, 10.0, false);
        assert_eq!(arrow, "▲");
        assert_eq!(text, "best yet");
        let (arrow, text, _) = metric_delta(12.0, 10.0, false);
        assert_eq!(arrow, "▼");
        assert_eq!(text, "+2 off best");
    }
}
