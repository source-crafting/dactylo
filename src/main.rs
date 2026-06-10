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
enum Screen {
    Setup,
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
    /// comparison summary. The summary is loaded BEFORE appending so it covers
    /// previous sessions only.
    fn build(settings: Settings, result: SessionResult, cancelled: bool) -> StatsView {
        let save = !cancelled;
        let (summary, warning) = match History::default_dir() {
            Some(dir) => {
                let history = History::new(dir);
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
        None => Screen::Setup,
    };
    // The stats screen to return to when Esc is pressed in setup. Set when the
    // user opens setup via `s`; cleared once a new session starts.
    let mut return_to: Option<StatsView> = None;

    loop {
        screen = match screen {
            Screen::Setup => match config_screen(terminal)? {
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
                ResultsAction::ChangeSettings => {
                    return_to = Some(view);
                    Screen::Setup
                }
                ResultsAction::Quit => return Ok(()),
            },
        };
    }
}

fn is_ctrl_c(code: KeyCode, modifiers: KeyModifiers) -> bool {
    code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL)
}

fn config_screen(terminal: &mut DefaultTerminal) -> io::Result<ConfigAction> {
    // Seed the screen from the last saved settings (or defaults on first run).
    let mut screen = ConfigScreen::from_settings(Settings::load());
    loop {
        terminal.draw(|f| ui::config::draw(f, &screen))?;
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
