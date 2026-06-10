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
use history::{History, Record};
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

fn run(terminal: &mut DefaultTerminal, cli_settings: Option<Settings>) -> io::Result<()> {
    let mut next_settings = cli_settings;
    loop {
        let settings = match next_settings.take() {
            Some(s) => s,
            None => match config_screen(terminal)? {
                Some(s) => s,
                None => return Ok(()),
            },
        };
        let (result, save) = match typing_screen(terminal, settings)? {
            TypingOutcome::Quit => return Ok(()),
            TypingOutcome::Completed(r) => (r, true),
            TypingOutcome::Cancelled(r) => (r, false),
        };
        match results_screen(terminal, settings, &result, save)? {
            ResultsAction::Restart => next_settings = Some(settings),
            ResultsAction::ChangeSettings => next_settings = None,
            ResultsAction::Quit => return Ok(()),
        }
    }
}

fn is_ctrl_c(code: KeyCode, modifiers: KeyModifiers) -> bool {
    code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL)
}

fn config_screen(terminal: &mut DefaultTerminal) -> io::Result<Option<Settings>> {
    let mut screen = ConfigScreen::from_settings(Settings::load());
    loop {
        terminal.draw(|f| ui::config::draw(f, &screen))?;
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if is_ctrl_c(key.code, key.modifiers) {
                return Ok(None);
            }
            match screen.handle_key(key.code) {
                ConfigAction::Confirm(s) => {
                    s.save();
                    return Ok(Some(s));
                }
                ConfigAction::Quit => return Ok(None),
                ConfigAction::None => {}
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

fn results_screen(
    terminal: &mut DefaultTerminal,
    settings: Settings,
    result: &SessionResult,
    save: bool,
) -> io::Result<ResultsAction> {
    // Summary is loaded BEFORE appending so the comparison covers previous
    // sessions only. Cancelled runs (save == false) are shown but not recorded.
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

    let cancelled = !save;
    loop {
        terminal.draw(|f| {
            ui::results::draw(
                f,
                result,
                summary.as_ref(),
                settings.level,
                settings.duration_secs,
                warning.as_deref(),
                cancelled,
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
