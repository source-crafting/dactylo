use ratatui::crossterm::event::KeyCode;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::config::{Settings, DURATIONS};

pub enum ConfigAction {
    None,
    Confirm(Settings),
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Duration,
    Level,
}

pub struct ConfigScreen {
    duration_idx: usize,
    level: u8,
    focus: Focus,
}

impl ConfigScreen {
    pub fn new() -> Self {
        ConfigScreen {
            duration_idx: 2, // 60s
            level: 3,
            focus: Focus::Duration,
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> ConfigAction {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => ConfigAction::Quit,
            KeyCode::Enter => ConfigAction::Confirm(Settings {
                duration_secs: DURATIONS[self.duration_idx],
                level: self.level,
            }),
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Duration => Focus::Level,
                    Focus::Level => Focus::Duration,
                };
                ConfigAction::None
            }
            KeyCode::Up | KeyCode::BackTab => {
                self.focus = Focus::Duration;
                ConfigAction::None
            }
            KeyCode::Down => {
                self.focus = Focus::Level;
                ConfigAction::None
            }
            KeyCode::Left => {
                match self.focus {
                    Focus::Duration => self.duration_idx = self.duration_idx.saturating_sub(1),
                    Focus::Level => self.level = self.level.saturating_sub(1).max(1),
                }
                ConfigAction::None
            }
            KeyCode::Right => {
                match self.focus {
                    Focus::Duration => {
                        self.duration_idx = (self.duration_idx + 1).min(DURATIONS.len() - 1)
                    }
                    Focus::Level => self.level = (self.level + 1).min(5),
                }
                ConfigAction::None
            }
            _ => ConfigAction::None,
        }
    }
}

impl Default for ConfigScreen {
    fn default() -> Self {
        Self::new()
    }
}

pub fn draw(frame: &mut Frame, screen: &ConfigScreen) {
    let area = frame.area();
    let block = Block::bordered().title(" dactylo — setup ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let dur_labels: Vec<String> = DURATIONS.iter().map(|d| format!("{d}s")).collect();
    let lvl_labels: Vec<String> = (1..=5).map(|l| l.to_string()).collect();

    let lines = vec![
        Line::default(),
        option_row(
            "duration",
            &dur_labels,
            screen.duration_idx,
            screen.focus == Focus::Duration,
        ),
        Line::default(),
        option_row(
            "level",
            &lvl_labels,
            (screen.level - 1) as usize,
            screen.focus == Focus::Level,
        ),
        Line::default(),
        Line::from(Span::styled(
            "  ←/→ change · tab switch · enter start · q quit",
            Style::new().fg(Color::DarkGray),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn option_row(label: &str, options: &[String], selected: usize, focused: bool) -> Line<'static> {
    let marker = if focused { " ▸ " } else { "   " };
    let mut spans = vec![
        Span::raw(marker.to_string()),
        Span::raw(format!("{label:<10}")),
    ];
    for (i, opt) in options.iter().enumerate() {
        let style = if i == selected {
            Style::new()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::DarkGray)
        };
        spans.push(Span::styled(format!(" {opt} "), style));
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyCode;

    #[test]
    fn defaults_are_60s_level_3() {
        let mut s = ConfigScreen::new();
        match s.handle_key(KeyCode::Enter) {
            ConfigAction::Confirm(set) => {
                assert_eq!(
                    set,
                    Settings {
                        duration_secs: 60,
                        level: 3
                    }
                )
            }
            _ => panic!("expected confirm"),
        }
    }

    #[test]
    fn arrows_change_values_and_clamp() {
        let mut s = ConfigScreen::new();
        s.handle_key(KeyCode::Right); // duration 60 -> 120
        s.handle_key(KeyCode::Right); // clamps at 120
        s.handle_key(KeyCode::Tab); // focus level
        s.handle_key(KeyCode::Left); // level 3 -> 2
        s.handle_key(KeyCode::Left); // 2 -> 1
        s.handle_key(KeyCode::Left); // clamps at 1
        match s.handle_key(KeyCode::Enter) {
            ConfigAction::Confirm(set) => {
                assert_eq!(
                    set,
                    Settings {
                        duration_secs: 120,
                        level: 1
                    }
                )
            }
            _ => panic!("expected confirm"),
        }
    }

    #[test]
    fn q_and_esc_quit() {
        let mut s = ConfigScreen::new();
        assert!(matches!(
            s.handle_key(KeyCode::Char('q')),
            ConfigAction::Quit
        ));
        assert!(matches!(s.handle_key(KeyCode::Esc), ConfigAction::Quit));
    }
}
