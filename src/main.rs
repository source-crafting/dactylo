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
        if !countdown(terminal)? {
            return Ok(());
        }
        let Some(result) = typing_screen(terminal, settings)? else {
            return Ok(()); // aborted: nothing recorded
        };
        match results_screen(terminal, settings, &result)? {
            ResultsAction::Retry => next_settings = Some(settings),
            ResultsAction::Quit => return Ok(()),
        }
    }
}

fn is_ctrl_c(code: KeyCode, modifiers: KeyModifiers) -> bool {
    code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL)
}

fn config_screen(terminal: &mut DefaultTerminal) -> io::Result<Option<Settings>> {
    let mut screen = ConfigScreen::new();
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
                ConfigAction::Confirm(s) => return Ok(Some(s)),
                ConfigAction::Quit => return Ok(None),
                ConfigAction::None => {}
            }
        }
    }
}

/// Returns false if the user aborted during the countdown.
fn countdown(terminal: &mut DefaultTerminal) -> io::Result<bool> {
    for n in (1..=3u8).rev() {
        terminal.draw(|f| ui::draw_countdown(f, n))?;
        let deadline = Instant::now() + Duration::from_millis(700);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            if event::poll(left)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press
                        && (key.code == KeyCode::Esc
                            || key.code == KeyCode::Char('q')
                            || is_ctrl_c(key.code, key.modifiers))
                    {
                        return Ok(false);
                    }
                }
            }
        }
    }
    Ok(true)
}

/// Returns None if the user aborted with Esc/Ctrl-C (no stats recorded).
fn typing_screen(
    terminal: &mut DefaultTerminal,
    settings: Settings,
) -> io::Result<Option<SessionResult>> {
    let mut session = Session::new(settings.level, Duration::from_secs(settings.duration_secs));
    loop {
        let now = Instant::now();
        session.tick(now);
        if session.is_finished(now) {
            return Ok(Some(session.results()));
        }
        terminal.draw(|f| {
            ui::typing::draw(f, &session, now, settings.level, settings.duration_secs)
        })?;
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Char(_) if is_ctrl_c(key.code, key.modifiers) => return Ok(None),
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
) -> io::Result<ResultsAction> {
    // Summary is loaded BEFORE appending so the comparison covers previous sessions only.
    let (summary, save_error) = match History::default_dir() {
        Some(dir) => {
            let history = History::new(dir);
            let summary = history.summary(settings.level);
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
            let save_error = history.append(&record).err().map(|e| e.to_string());
            (summary, save_error)
        }
        None => (None, Some("could not locate home directory".to_string())),
    };

    loop {
        terminal.draw(|f| {
            ui::results::draw(
                f,
                result,
                summary.as_ref(),
                settings.level,
                settings.duration_secs,
                save_error.as_deref(),
            )
        })?;
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if is_ctrl_c(key.code, key.modifiers) {
                return Ok(ResultsAction::Quit);
            }
            match key.code {
                KeyCode::Char('r') => return Ok(ResultsAction::Retry),
                KeyCode::Char('q') | KeyCode::Esc => return Ok(ResultsAction::Quit),
                _ => {}
            }
        }
    }
}
