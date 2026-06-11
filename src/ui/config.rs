use ratatui::crossterm::event::KeyCode;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::config::{Settings, DURATIONS};

pub enum ConfigAction {
    Confirm(Settings),
    /// Esc — go back (to the stats screen if there is one, else quit).
    Back,
    /// `q` — quit the app.
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
    /// Build the setup screen pre-filled from `settings`. A `duration_secs`
    /// not in `DURATIONS` falls back to the 60s slot; `level` is clamped to 1..=5.
    pub fn from_settings(settings: Settings) -> Self {
        let duration_idx = DURATIONS
            .iter()
            .position(|&d| d == settings.duration_secs)
            .unwrap_or(2); // 60s
        ConfigScreen {
            duration_idx,
            level: settings.level.clamp(1, 5),
            focus: Focus::Duration,
        }
    }

    /// Map a keypress to an action, mutating focus/selection for navigation
    /// keys. Returns `None` for keys that only change state (or do nothing).
    /// (Ctrl-C is handled by the caller before this is consulted.)
    pub fn handle_key(&mut self, code: KeyCode) -> Option<ConfigAction> {
        match code {
            KeyCode::Esc => Some(ConfigAction::Back),
            KeyCode::Char('q') => Some(ConfigAction::Quit),
            KeyCode::Enter => Some(ConfigAction::Confirm(Settings {
                duration_secs: DURATIONS[self.duration_idx],
                level: self.level,
            })),
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Duration => Focus::Level,
                    Focus::Level => Focus::Duration,
                };
                None
            }
            KeyCode::Up | KeyCode::BackTab => {
                self.focus = Focus::Duration;
                None
            }
            KeyCode::Down => {
                self.focus = Focus::Level;
                None
            }
            KeyCode::Left => {
                match self.focus {
                    Focus::Duration => self.duration_idx = self.duration_idx.saturating_sub(1),
                    Focus::Level => self.level = self.level.saturating_sub(1).max(1),
                }
                None
            }
            KeyCode::Right => {
                match self.focus {
                    Focus::Duration => {
                        self.duration_idx = (self.duration_idx + 1).min(DURATIONS.len() - 1)
                    }
                    Focus::Level => self.level = (self.level + 1).min(5),
                }
                None
            }
            _ => None,
        }
    }
}

impl Default for ConfigScreen {
    fn default() -> Self {
        Self::from_settings(Settings::default())
    }
}

/// Draw the setup screen. `can_return` is true when Esc would return to a stats
/// screen (setup opened via `s`), false when Esc just quits (first-run launch).
pub fn draw(frame: &mut Frame, screen: &ConfigScreen, can_return: bool) {
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
            if can_return {
                "  ←/→ change · tab switch · enter start · esc back · q quit"
            } else {
                "  ←/→ change · tab switch · enter start · esc/q quit"
            },
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
        let mut s = ConfigScreen::default();
        match s.handle_key(KeyCode::Enter) {
            Some(ConfigAction::Confirm(set)) => {
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
        let mut s = ConfigScreen::default();
        s.handle_key(KeyCode::Right); // duration 60 -> 120
        s.handle_key(KeyCode::Right); // clamps at 120
        s.handle_key(KeyCode::Tab); // focus level
        s.handle_key(KeyCode::Left); // level 3 -> 2
        s.handle_key(KeyCode::Left); // 2 -> 1
        s.handle_key(KeyCode::Left); // clamps at 1
        match s.handle_key(KeyCode::Enter) {
            Some(ConfigAction::Confirm(set)) => {
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
    fn q_quits_and_esc_goes_back() {
        let mut s = ConfigScreen::default();
        assert!(matches!(
            s.handle_key(KeyCode::Char('q')),
            Some(ConfigAction::Quit)
        ));
        assert!(matches!(
            s.handle_key(KeyCode::Esc),
            Some(ConfigAction::Back)
        ));
    }

    #[test]
    fn navigation_keys_return_none() {
        let mut s = ConfigScreen::default();
        assert!(s.handle_key(KeyCode::Tab).is_none());
        assert!(s.handle_key(KeyCode::Left).is_none());
        assert!(s.handle_key(KeyCode::Right).is_none());
        assert!(s.handle_key(KeyCode::Up).is_none());
        assert!(s.handle_key(KeyCode::Down).is_none());
    }

    #[test]
    fn from_settings_maps_duration_and_level() {
        let mut s = ConfigScreen::from_settings(Settings {
            duration_secs: 15,
            level: 4,
        });
        match s.handle_key(KeyCode::Enter) {
            Some(ConfigAction::Confirm(set)) => assert_eq!(
                set,
                Settings {
                    duration_secs: 15,
                    level: 4
                }
            ),
            _ => panic!("expected confirm"),
        }
    }

    #[test]
    fn from_settings_unknown_duration_falls_back_to_60() {
        let mut s = ConfigScreen::from_settings(Settings {
            duration_secs: 45,
            level: 3,
        });
        match s.handle_key(KeyCode::Enter) {
            Some(ConfigAction::Confirm(set)) => assert_eq!(set.duration_secs, 60),
            _ => panic!("expected confirm"),
        }
    }

    #[test]
    fn from_settings_clamps_level() {
        let mut s = ConfigScreen::from_settings(Settings {
            duration_secs: 60,
            level: 9,
        });
        match s.handle_key(KeyCode::Enter) {
            Some(ConfigAction::Confirm(set)) => assert_eq!(set.level, 5),
            _ => panic!("expected confirm"),
        }
    }
}
