use ratatui::crossterm::event::KeyCode;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::history::LevelSummary;
use crate::session::SessionResult;

pub enum ResultsAction {
    Restart,
    ChangeSettings,
    Quit,
}

/// Map a results-screen keypress to an action. Returns `None` for keys that do
/// nothing here. (Ctrl-C is handled by the caller before this is consulted.)
pub fn handle_key(code: KeyCode) -> Option<ResultsAction> {
    match code {
        KeyCode::Enter => Some(ResultsAction::Restart),
        KeyCode::Char('s') | KeyCode::Char('S') => Some(ResultsAction::ChangeSettings),
        KeyCode::Char('q') | KeyCode::Esc => Some(ResultsAction::Quit),
        _ => None,
    }
}

pub fn draw(
    frame: &mut Frame,
    result: &SessionResult,
    summary: Option<&LevelSummary>,
    level: u8,
    duration_secs: u64,
    warning: Option<&str>,
    cancelled: bool,
) {
    let area = frame.area();
    let title = if cancelled {
        format!(" results — level {level} · {duration_secs}s · cancelled ")
    } else {
        format!(" results — level {level} · {duration_secs}s ")
    };
    let block = Block::bordered().title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![
        Line::default(),
        Line::from(Span::styled(
            format!("  wpm          {:>6.1}", result.wpm),
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("  raw wpm      {:>6.1}", result.raw_wpm)),
        Line::from(format!("  accuracy     {:>5.1}%", result.accuracy)),
        Line::from(format!("  errors       {:>6}", result.errors)),
        Line::from(format!("  consistency  {:>5.1}%", result.consistency)),
        Line::default(),
    ];

    if cancelled {
        lines.push(Line::from(Span::styled(
            "  session cancelled — not saved to history",
            Style::new().fg(Color::Yellow),
        )));
        lines.push(Line::default());
    }

    match summary {
        Some(sum) => {
            lines.push(Line::from(format!(
                "  vs your {} previous level-{} session(s):",
                sum.count, level
            )));
            lines.push(delta_line("wpm", result.wpm, sum.avg_wpm, sum.best_wpm, ""));
            lines.push(delta_line(
                "acc",
                result.accuracy,
                sum.avg_accuracy,
                sum.best_accuracy,
                "%",
            ));
        }
        None => lines.push(Line::from("  first session at this level — no history yet")),
    }

    if let Some(err) = warning {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            format!("  warning: {err}"),
            Style::new().fg(Color::Yellow),
        )));
    }

    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "  [enter] restart   [s] settings   [q/esc] quit",
        Style::new().fg(Color::DarkGray),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyCode;

    #[test]
    fn enter_restarts() {
        assert!(matches!(
            handle_key(KeyCode::Enter),
            Some(ResultsAction::Restart)
        ));
    }

    #[test]
    fn s_opens_settings() {
        assert!(matches!(
            handle_key(KeyCode::Char('s')),
            Some(ResultsAction::ChangeSettings)
        ));
        assert!(matches!(
            handle_key(KeyCode::Char('S')),
            Some(ResultsAction::ChangeSettings)
        ));
    }

    #[test]
    fn q_and_esc_quit() {
        assert!(matches!(
            handle_key(KeyCode::Char('q')),
            Some(ResultsAction::Quit)
        ));
        assert!(matches!(
            handle_key(KeyCode::Esc),
            Some(ResultsAction::Quit)
        ));
    }

    #[test]
    fn r_is_ignored() {
        assert!(handle_key(KeyCode::Char('r')).is_none());
    }
}
