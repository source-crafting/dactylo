mod config;
mod history;
mod jsonl;
mod mistakes;
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
use history::{summary_of, History, LevelSummary, Record};
use mistakes::{MistakeLog, MistakeRecord, MistakeTally, Sort, WeaknessProfile};
use session::{Session, SessionResult};
use ui::config::{ConfigAction, ConfigScreen};
use ui::results::{ResultsAction, ResultsScreen};
use ui::weaknesses::{WeaknessAction, WeaknessScreen};
use words::{PracticePool, WordPool, WordStream};

/// The current UTC time as an RFC3339 second-precision string (the timestamp
/// format used in both history.jsonl and mistakes.jsonl).
fn now_ts() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

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
    Completed(SessionResult, MistakeTally),
    Cancelled(SessionResult, MistakeTally),
    Quit,
}

/// The screen currently being shown. The app is a loop over these states.
/// `Setup` carries the settings to pre-fill the screen with.
enum Screen {
    Setup(Settings),
    Typing(Settings),
    Practice(Settings, Box<dyn WordStream>),
    Stats(ResultsScreen),
    Weaknesses(WeaknessScreen),
}

/// History I/O result for a finished session, gathered once.
struct Snapshot {
    /// All records after this run was (conditionally) appended.
    records: Vec<Record>,
    /// Played level's summary BEFORE the append — for the this-session delta.
    prev_summary: Option<LevelSummary>,
    warning: Option<String>,
}

/// Record the result to history (unless `cancelled`) and build the snapshot in a
/// single file read. `history` is `None` only when the home directory can't be
/// located. The pre-append records are captured first so the this-session delta
/// compares against previous sessions only; the just-saved record is then pushed
/// in-memory rather than re-reading the file.
fn record_and_snapshot(
    history: Option<&History>,
    settings: Settings,
    result: SessionResult,
    cancelled: bool,
) -> Snapshot {
    match history {
        Some(history) => {
            let (mut records, skipped) = history.load_with_skipped();
            // Summary over the pre-append records, so the "vs previous" delta
            // excludes the run just played.
            let prev_summary = summary_of(&records, settings.level);
            let mut warnings: Vec<String> = Vec::new();
            if !cancelled {
                let record = Record {
                    ts: now_ts(),
                    duration: settings.duration_secs,
                    level: settings.level,
                    wpm: result.wpm,
                    raw_wpm: result.raw_wpm,
                    accuracy: result.accuracy,
                    errors: result.errors,
                    consistency: result.consistency,
                    chars: result.chars,
                };
                match history.append(&record) {
                    Ok(()) => records.push(record),
                    Err(e) => warnings.push(format!("result not saved: {e}")),
                }
            }
            if skipped > 0 {
                warnings.push(format!("{skipped} corrupt history line(s) ignored"));
            }
            let warning = (!warnings.is_empty()).then(|| warnings.join(" · "));
            Snapshot {
                records,
                prev_summary,
                warning,
            }
        }
        None if !cancelled => Snapshot {
            records: Vec::new(),
            prev_summary: None,
            warning: Some("result not saved: could not locate home directory".to_string()),
        },
        None => Snapshot {
            records: Vec::new(),
            prev_summary: None,
            warning: None,
        },
    }
}

/// Append the run's mistake detail to `mistakes.jsonl` (best-effort; a failure
/// is silently ignored — mistake logging must never interrupt the app). Skips
/// empty tallies (a run with no keystrokes). `practice` runs and cancelled runs
/// are logged here but never reach `history.jsonl`.
fn record_mistakes(log: Option<&MistakeLog>, tally: &MistakeTally, level: u8, practice: bool) {
    if tally.is_empty() {
        return;
    }
    if let Some(log) = log {
        let _ = log.append(&MistakeRecord::from_tally(tally, level, practice, now_ts()));
    }
}

/// Record the session and build the dashboard screen for it.
fn results_screen_for(settings: Settings, result: SessionResult, cancelled: bool) -> ResultsScreen {
    let history = History::default_dir().map(History::new);
    let snap = record_and_snapshot(history.as_ref(), settings, result, cancelled);
    ResultsScreen::new(
        snap.records,
        settings,
        result,
        cancelled,
        snap.prev_summary,
        snap.warning,
    )
}

/// Load all readable mistake records (empty if the file/dir is unavailable).
fn load_mistake_records() -> Vec<MistakeRecord> {
    History::default_dir()
        .map(MistakeLog::new)
        .map(|log| log.load_with_skipped().0)
        .unwrap_or_default()
}

/// Read mistakes.jsonl and build the explorer screen for `level`.
fn weakness_screen_for(level: u8) -> WeaknessScreen {
    let recs = load_mistake_records();
    WeaknessScreen::new(WeaknessProfile::from_records(&recs, Sort::Rate), level)
}

/// Build a practice source from an already-computed profile, falling back to the
/// normal level pool when there's no usable weakness signal.
fn practice_source_from(profile: &WeaknessProfile, level: u8) -> Box<dyn WordStream> {
    match PracticePool::build(profile, level) {
        Some(p) => Box::new(p),
        None => Box::new(WordPool::for_level(level)),
    }
}

/// Build a practice source by reading the latest profile from disk (used by the
/// practice-results restart path, which needs fresh data after a drill).
fn practice_source(level: u8) -> Box<dyn WordStream> {
    let profile = WeaknessProfile::from_records(&load_mistake_records(), Sort::Rate);
    practice_source_from(&profile, level)
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
    let mut return_to: Option<ResultsScreen> = None;
    let mistake_log = History::default_dir().map(MistakeLog::new);

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
                TypingOutcome::Completed(r, tally) => {
                    record_mistakes(mistake_log.as_ref(), &tally, settings.level, false);
                    Screen::Stats(results_screen_for(settings, r, false))
                }
                TypingOutcome::Cancelled(r, tally) => {
                    record_mistakes(mistake_log.as_ref(), &tally, settings.level, false);
                    Screen::Stats(results_screen_for(settings, r, true))
                }
            },
            Screen::Practice(settings, source) => {
                match practice_screen(terminal, settings, source)? {
                    TypingOutcome::Quit => return Ok(()),
                    TypingOutcome::Completed(r, tally) => {
                        record_mistakes(mistake_log.as_ref(), &tally, settings.level, true);
                        Screen::Stats(ResultsScreen::new_practice(settings, r, false))
                    }
                    TypingOutcome::Cancelled(r, tally) => {
                        record_mistakes(mistake_log.as_ref(), &tally, settings.level, true);
                        Screen::Stats(ResultsScreen::new_practice(settings, r, true))
                    }
                }
            }
            Screen::Stats(mut screen) => match stats_screen(terminal, &mut screen)? {
                ResultsAction::Restart if screen.is_practice() => {
                    let s = screen.settings();
                    Screen::Practice(s, practice_source(s.level))
                }
                ResultsAction::Restart => Screen::Typing(screen.settings()),
                // Pre-fill setup with the active session's settings (not just
                // the saved file), and remember this screen to return to.
                ResultsAction::ChangeSettings => {
                    let seed = screen.settings();
                    return_to = Some(screen);
                    Screen::Setup(seed)
                }
                ResultsAction::Weaknesses => {
                    let level = screen.settings().level;
                    return_to = Some(screen);
                    Screen::Weaknesses(weakness_screen_for(level))
                }
                ResultsAction::Quit => return Ok(()),
            },
            Screen::Weaknesses(mut screen) => match weaknesses_screen(terminal, &mut screen)? {
                WeaknessAction::Back => match return_to.take() {
                    Some(view) => Screen::Stats(view),
                    None => return Ok(()),
                },
                WeaknessAction::Practice => {
                    // Launching practice leaves the results lineage; capture the
                    // duration first, then drop the prior results screen — it is
                    // no longer something to return to.
                    let duration_secs = return_to
                        .as_ref()
                        .map(|r| r.settings().duration_secs)
                        .unwrap_or(Settings::default().duration_secs);
                    return_to = None;
                    let level = screen.level();
                    let source = practice_source_from(screen.profile(), level);
                    Screen::Practice(
                        Settings {
                            level,
                            duration_secs,
                        },
                        source,
                    )
                }
                WeaknessAction::Quit => return Ok(()),
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
    typing_loop(terminal, &mut session, settings)
}

/// Like `typing_screen` but over a prepared practice word source.
fn practice_screen(
    terminal: &mut DefaultTerminal,
    settings: Settings,
    source: Box<dyn WordStream>,
) -> io::Result<TypingOutcome> {
    let mut session = Session::with_source(source, Duration::from_secs(settings.duration_secs));
    typing_loop(terminal, &mut session, settings)
}

fn typing_loop(
    terminal: &mut DefaultTerminal,
    session: &mut Session,
    settings: Settings,
) -> io::Result<TypingOutcome> {
    loop {
        let now = Instant::now();
        session.tick(now);
        if session.is_finished(now) {
            return Ok(TypingOutcome::Completed(session.results(), session.tally()));
        }
        terminal
            .draw(|f| ui::typing::draw(f, session, now, settings.level, settings.duration_secs))?;
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if is_ctrl_c(key.code, key.modifiers) {
                    return Ok(TypingOutcome::Quit);
                }
                match key.code {
                    KeyCode::Esc => {
                        return Ok(TypingOutcome::Cancelled(
                            session.results_at(Instant::now()),
                            session.tally(),
                        ))
                    }
                    KeyCode::Backspace => session.backspace(),
                    KeyCode::Char(c) => session.keystroke(c, Instant::now()),
                    _ => {}
                }
            }
        }
    }
}

/// Draw the dashboard and wait for the user's next action. Pure display + level
/// navigation — all history I/O already happened in [`results_screen_for`], so
/// this can be re-entered (e.g. returning from setup via Esc) without side
/// effects, and tab browsing state is preserved across re-entry.
fn stats_screen(
    terminal: &mut DefaultTerminal,
    screen: &mut ResultsScreen,
) -> io::Result<ResultsAction> {
    loop {
        terminal.draw(|f| screen.draw(f))?;
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if is_ctrl_c(key.code, key.modifiers) {
                return Ok(ResultsAction::Quit);
            }
            if let Some(action) = screen.handle_key(key.code) {
                return Ok(action);
            }
        }
    }
}

fn weaknesses_screen(
    terminal: &mut DefaultTerminal,
    screen: &mut WeaknessScreen,
) -> io::Result<WeaknessAction> {
    loop {
        terminal.draw(|f| screen.draw(f))?;
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if is_ctrl_c(key.code, key.modifiers) {
                return Ok(WeaknessAction::Quit);
            }
            if let Some(action) = screen.handle_key(key.code) {
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
        let snap = record_and_snapshot(Some(&history), settings(), sample_result(), false);
        assert_eq!(history.load_with_skipped().0.len(), 1);
        assert_eq!(snap.records.len(), 1);
    }

    #[test]
    fn cancelled_run_is_not_appended() {
        let (_dir, history) = temp_history();
        let snap = record_and_snapshot(Some(&history), settings(), sample_result(), true);
        assert!(history.load_with_skipped().0.is_empty());
        assert!(snap.records.is_empty());
    }

    #[test]
    fn prev_summary_reflects_previous_sessions_only() {
        let (_dir, history) = temp_history();
        record_and_snapshot(Some(&history), settings(), sample_result(), false);
        record_and_snapshot(Some(&history), settings(), sample_result(), false);
        // Third completed run: prev_summary covers the 2 priors; snapshot has 3.
        let snap = record_and_snapshot(Some(&history), settings(), sample_result(), false);
        assert_eq!(snap.prev_summary.unwrap().count, 2);
        assert_eq!(snap.records.len(), 3);
    }

    #[test]
    fn corrupt_history_warns_on_cancelled_run_too() {
        use std::io::Write;
        let (dir, history) = temp_history();
        record_and_snapshot(Some(&history), settings(), sample_result(), false);
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.path().join("history.jsonl"))
            .unwrap();
        writeln!(f, "not json").unwrap();
        let snap = record_and_snapshot(Some(&history), settings(), sample_result(), true);
        assert!(snap.warning.as_deref().unwrap().contains("corrupt"));
        assert_eq!(history.load_with_skipped().0.len(), 1); // cancelled run appended nothing
    }

    #[test]
    fn missing_home_warns_only_when_saving() {
        let completed = record_and_snapshot(None, settings(), sample_result(), false);
        assert!(completed.warning.is_some());
        let cancelled = record_and_snapshot(None, settings(), sample_result(), true);
        assert!(cancelled.warning.is_none());
    }

    #[test]
    fn mistakes_are_logged_for_normal_and_practice_runs() {
        use crate::mistakes::{MistakeLog, MistakeTally};
        let dir = tempfile::tempdir().unwrap();
        let log = MistakeLog::new(dir.path().to_path_buf());
        let mut tally = MistakeTally::default();
        tally.keys.insert('b', (10, 4));
        // normal completed run
        record_mistakes(Some(&log), &tally, 3, false);
        // practice run
        record_mistakes(Some(&log), &tally, 3, true);
        let (recs, _) = log.load_with_skipped();
        assert_eq!(recs.len(), 2);
        assert!(!recs[0].practice && recs[1].practice);
    }

    #[test]
    fn empty_tally_is_not_logged() {
        use crate::mistakes::{MistakeLog, MistakeTally};
        let dir = tempfile::tempdir().unwrap();
        let log = MistakeLog::new(dir.path().to_path_buf());
        record_mistakes(Some(&log), &MistakeTally::default(), 3, false);
        assert!(log.load_with_skipped().0.is_empty());
    }
}
