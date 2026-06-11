mod config;
mod history;
mod session;
mod stats;
mod ui;
mod words;

use std::io;
use std::time::{Duration, Instant};

use clap::Parser;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use config::{Cli, Settings};
use history::{History, LevelSummary, Record};
use session::{Session, SessionResult};
use ui::config::{ConfigAction, ConfigScreen};
use ui::results::ResultsAction;

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    let cli_settings = match Settings::from_cli(&cli) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, cli_settings);
    ratatui::restore();
    result
}

enum TypingOutcome {
    Completed(SessionResult),
    Cancelled(SessionResult),
    Quit,
}

/// The screen currently being shown. The app is a loop over these states.
/// `Setup` carries the settings to pre-fill the screen with.
enum Screen {
    Setup(Settings),
    Typing(Settings),
    Stats(StatsView),
}

/// A finished session's stats, ready to display. History I/O (recording the
/// result and loading the comparison summary) happens once, in [`StatsView::build`],
/// so the stats screen can be re-shown — e.g. after bouncing to setup and back —
/// without re-saving or shifting the "vs previous sessions" comparison.
struct StatsView {
    settings: Settings,
    result: SessionResult,
    summary: Option<LevelSummary>,
    warning: Option<String>,
    cancelled: bool,
}

impl StatsView {
    /// Record the result to history (unless `cancelled`) and gather the
    /// comparison summary, using the default history location.
    fn build(settings: Settings, result: SessionResult, cancelled: bool) -> StatsView {
        let history = History::default_dir().map(History::new);
        Self::build_with(history.as_ref(), settings, result, cancelled)
    }

    /// Core of [`build`], parameterized over the history store so its
    /// save/skip and summary-before-append behavior is testable. `history` is
    /// `None` only when the home directory cannot be located. The summary is
    /// loaded BEFORE appending so it covers previous sessions only.
    fn build_with(
        history: Option<&History>,
        settings: Settings,
        result: SessionResult,
        cancelled: bool,
    ) -> StatsView {
        let save = !cancelled;
        let (summary, warning) = match history {
            Some(history) => {
                let summary = history.summary(settings.level);
                let mut warnings: Vec<String> = Vec::new();
                if save {
                    let record = Record {
                        ts: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                        duration: settings.duration_secs,
                        level: settings.level,
                        wpm: result.wpm,
                        raw_wpm: result.raw_wpm,
                        accuracy: result.accuracy,
                        errors: result.errors,
                        consistency: result.consistency,
                        chars: result.chars,
                    };
                    if let Err(e) = history.append(&record) {
                        warnings.push(format!("result not saved: {e}"));
                    }
                }
                // Reports the integrity of the history file we just read for
                // the comparison summary — surfaced on cancelled runs too,
                // independent of whether this result was saved.
                let skipped = history.skipped_lines();
                if skipped > 0 {
                    warnings.push(format!("{skipped} corrupt history line(s) ignored"));
                }
                let warning = (!warnings.is_empty()).then(|| warnings.join(" · "));
                (summary, warning)
            }
            None if save => (
                None,
                Some("result not saved: could not locate home directory".to_string()),
            ),
            None => (None, None),
        };
        StatsView {
            settings,
            result,
            summary,
            warning,
            cancelled,
        }
    }
}

fn run(terminal: &mut DefaultTerminal, cli_settings: Option<Settings>) -> io::Result<()> {
    // Launch straight into a session when the settings are already known — from
    // CLI flags, or a previously saved file. Only first-time users (no saved
    // settings) see the setup screen at startup.
    let mut screen = match cli_settings.or_else(Settings::load_existing) {
        Some(s) => Screen::Typing(s),
        // Reaching here means no readable settings file exists, so the seed is
        // necessarily the defaults — no point re-reading the disk.
        None => Screen::Setup(Settings::default()),
    };
    // The stats screen to return to when Esc is pressed in setup. Set when the
    // user opens setup via `s`; cleared once a new session starts.
    let mut return_to: Option<StatsView> = None;

    loop {
        screen = match screen {
            Screen::Setup(seed) => match config_screen(terminal, seed, return_to.is_some())? {
                ConfigAction::Confirm(s) => {
                    return_to = None;
                    Screen::Typing(s)
                }
                // Esc: go back to the stats screen we came from, or quit if
                // there is none (e.g. the first-run setup at startup).
                ConfigAction::Back => match return_to.take() {
                    Some(view) => Screen::Stats(view),
                    None => return Ok(()),
                },
                ConfigAction::Quit => return Ok(()),
            },
            Screen::Typing(settings) => match typing_screen(terminal, settings)? {
                TypingOutcome::Quit => return Ok(()),
                TypingOutcome::Completed(r) => Screen::Stats(StatsView::build(settings, r, false)),
                TypingOutcome::Cancelled(r) => Screen::Stats(StatsView::build(settings, r, true)),
            },
            Screen::Stats(view) => match stats_screen(terminal, &view)? {
                ResultsAction::Restart => Screen::Typing(view.settings),
                // Pre-fill setup with the active session's settings (not just
                // the saved file), and remember this screen to return to.
                ResultsAction::ChangeSettings => {
                    let seed = view.settings;
                    return_to = Some(view);
                    Screen::Setup(seed)
                }
                ResultsAction::Quit => return Ok(()),
            },
        };
    }
}

fn is_ctrl_c(code: KeyCode, modifiers: KeyModifiers) -> bool {
    code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL)
}

fn config_screen(
    terminal: &mut DefaultTerminal,
    seed: Settings,
    can_return: bool,
) -> io::Result<ConfigAction> {
    let mut screen = ConfigScreen::from_settings(seed);
    loop {
        terminal.draw(|f| ui::config::draw(f, &screen, can_return))?;
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if is_ctrl_c(key.code, key.modifiers) {
                return Ok(ConfigAction::Quit);
            }
            if let Some(action) = screen.handle_key(key.code) {
                if let ConfigAction::Confirm(s) = &action {
                    s.save();
                }
                return Ok(action);
            }
        }
    }
}

fn typing_screen(terminal: &mut DefaultTerminal, settings: Settings) -> io::Result<TypingOutcome> {
    let mut session = Session::new(settings.level, Duration::from_secs(settings.duration_secs));
    loop {
        let now = Instant::now();
        session.tick(now);
        if session.is_finished(now) {
            return Ok(TypingOutcome::Completed(session.results()));
        }
        terminal
            .draw(|f| ui::typing::draw(f, &session, now, settings.level, settings.duration_secs))?;
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if is_ctrl_c(key.code, key.modifiers) {
                    return Ok(TypingOutcome::Quit);
                }
                match key.code {
                    // Esc cancels: show partial stats (over elapsed time), not saved.
                    KeyCode::Esc => {
                        return Ok(TypingOutcome::Cancelled(session.results_at(Instant::now())))
                    }
                    KeyCode::Backspace => session.backspace(),
                    // Fresh Instant: timestamp at receipt, not at render start.
                    KeyCode::Char(c) => session.keystroke(c, Instant::now()),
                    _ => {}
                }
            }
        }
    }
}

/// Draw the stats screen and wait for the user's next action. Pure display —
/// all history I/O already happened in [`StatsView::build`], so this can be
/// re-entered (e.g. returning from setup via Esc) without side effects.
fn stats_screen(terminal: &mut DefaultTerminal, view: &StatsView) -> io::Result<ResultsAction> {
    loop {
        terminal.draw(|f| {
            ui::results::draw(
                f,
                &view.result,
                view.summary.as_ref(),
                view.settings.level,
                view.settings.duration_secs,
                view.warning.as_deref(),
                view.cancelled,
            )
        })?;
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if is_ctrl_c(key.code, key.modifiers) {
                return Ok(ResultsAction::Quit);
            }
            if let Some(action) = ui::results::handle_key(key.code) {
                return Ok(action);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_result() -> SessionResult {
        SessionResult {
            wpm: 50.0,
            raw_wpm: 55.0,
            accuracy: 96.0,
            errors: 3,
            consistency: 90.0,
            chars: 250,
        }
    }

    fn settings() -> Settings {
        Settings {
            duration_secs: 60,
            level: 3,
        }
    }

    fn temp_history() -> (tempfile::TempDir, History) {
        let dir = tempfile::tempdir().unwrap();
        let history = History::new(dir.path().to_path_buf());
        (dir, history)
    }

    #[test]
    fn completed_run_is_appended_once() {
        let (_dir, history) = temp_history();
        let view = StatsView::build_with(Some(&history), settings(), sample_result(), false);
        assert!(!view.cancelled);
        assert_eq!(history.load().len(), 1);
    }

    #[test]
    fn cancelled_run_is_not_appended() {
        let (_dir, history) = temp_history();
        let view = StatsView::build_with(Some(&history), settings(), sample_result(), true);
        assert!(view.cancelled);
        assert!(history.load().is_empty());
    }

    #[test]
    fn summary_reflects_previous_sessions_only() {
        let (_dir, history) = temp_history();
        // Two prior completed level-3 sessions.
        StatsView::build_with(Some(&history), settings(), sample_result(), false);
        StatsView::build_with(Some(&history), settings(), sample_result(), false);
        // A third: its comparison summary must cover the 2 priors, not itself.
        let view = StatsView::build_with(Some(&history), settings(), sample_result(), false);
        assert_eq!(view.summary.unwrap().count, 2);
        // ...and the third is now persisted.
        assert_eq!(history.load().len(), 3);
    }

    #[test]
    fn corrupt_history_warns_on_cancelled_run_too() {
        use std::io::Write;
        let (dir, history) = temp_history();
        // One valid prior record, then a malformed line appended directly.
        StatsView::build_with(Some(&history), settings(), sample_result(), false);
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.path().join("history.jsonl"))
            .unwrap();
        writeln!(f, "not json").unwrap();
        // A cancelled run still reads history for the comparison, so the
        // corruption notice surfaces even though nothing new is saved.
        let view = StatsView::build_with(Some(&history), settings(), sample_result(), true);
        assert!(view.cancelled);
        assert!(view.warning.as_deref().unwrap().contains("corrupt"));
        assert_eq!(history.load().len(), 1); // cancelled run appended nothing
    }

    #[test]
    fn missing_home_warns_only_when_saving() {
        let completed = StatsView::build_with(None, settings(), sample_result(), false);
        assert!(completed.summary.is_none());
        assert!(completed.warning.is_some());

        let cancelled = StatsView::build_with(None, settings(), sample_result(), true);
        assert!(cancelled.warning.is_none());
    }
}
