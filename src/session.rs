use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::mistakes::MistakeTally;
use crate::stats;
use crate::words::{WordPool, WordStream};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CharState {
    Untyped,
    Correct,
    Wrong,
}

#[derive(Debug, Clone, Copy)]
pub struct SessionResult {
    pub wpm: f64,
    pub raw_wpm: f64,
    pub accuracy: f64,
    pub errors: usize,
    pub consistency: f64,
    pub chars: usize,
}

/// Minimum number of target chars kept ahead of the cursor. Sized so the typing
/// screen always has enough text to fill three lines at the default column width.
const LOOKAHEAD: usize = 256;

pub struct Session {
    source: Box<dyn WordStream>,
    target: Vec<char>,
    states: Vec<CharState>,
    /// Per-position flag: set when any wrong keystroke lands here; never cleared
    /// by backspace. Used to detect fumbled words.
    ever_wrong: Vec<bool>,
    /// (start, end, text) of every generated word, in target order.
    word_spans: Vec<(usize, usize, String)>,
    /// expected char -> (occurrences, wrong), keystroke-based; spaces excluded.
    key_tally: BTreeMap<char, (u32, u32)>,
    cursor: usize,
    errors: usize,
    keystrokes: usize,
    duration: Duration,
    started_at: Option<Instant>,
    last_word: Option<String>,
    wpm_samples: Vec<f64>,
    sampled_secs: u64,
}

impl Session {
    pub fn new(level: u8, duration: Duration) -> Self {
        Self::with_source(Box::new(WordPool::for_level(level)), duration)
    }

    /// Build a session over an arbitrary word source (e.g. the practice pool).
    pub fn with_source(source: Box<dyn WordStream>, duration: Duration) -> Self {
        let mut session = Session {
            source,
            target: Vec::new(),
            states: Vec::new(),
            ever_wrong: Vec::new(),
            word_spans: Vec::new(),
            key_tally: BTreeMap::new(),
            cursor: 0,
            errors: 0,
            keystrokes: 0,
            duration,
            started_at: None,
            last_word: None,
            wpm_samples: Vec::new(),
            sampled_secs: 0,
        };
        session.extend_target();
        session
    }

    fn extend_target(&mut self) {
        while self.target.len() < self.cursor + LOOKAHEAD {
            if !self.target.is_empty() {
                self.target.push(' ');
                self.states.push(CharState::Untyped);
                self.ever_wrong.push(false);
            }
            let word = self.source.next_word(self.last_word.as_deref());
            let start = self.target.len();
            for c in word.chars() {
                self.target.push(c);
                self.states.push(CharState::Untyped);
                self.ever_wrong.push(false);
            }
            let end = self.target.len();
            self.word_spans.push((start, end, word.clone()));
            self.last_word = Some(word);
        }
    }

    pub fn keystroke(&mut self, c: char, now: Instant) {
        if self.is_finished(now) {
            return;
        }
        if self.started_at.is_none() {
            self.started_at = Some(now);
        }
        self.keystrokes += 1;
        let expected = self.target[self.cursor];
        let wrong = c != expected;
        if wrong {
            self.states[self.cursor] = CharState::Wrong;
            self.ever_wrong[self.cursor] = true;
            self.errors += 1;
        } else {
            self.states[self.cursor] = CharState::Correct;
        }
        if expected != ' ' {
            let e = self.key_tally.entry(expected).or_insert((0, 0));
            e.0 += 1;
            if wrong {
                e.1 += 1;
            }
        }
        self.cursor += 1;
        self.extend_target();
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.states[self.cursor] = CharState::Untyped;
        }
    }

    pub fn elapsed(&self, now: Instant) -> Duration {
        self.started_at.map_or(Duration::ZERO, |start| {
            now.duration_since(start).min(self.duration)
        })
    }

    pub fn remaining(&self, now: Instant) -> Duration {
        self.duration - self.elapsed(now)
    }

    pub fn is_finished(&self, now: Instant) -> bool {
        self.started_at.is_some() && self.elapsed(now) >= self.duration
    }

    /// Record one net-WPM sample per elapsed whole second. Call every event-loop pass.
    /// NOTE: if called less than once per second (event-loop stall, system sleep),
    /// catch-up samples all use the current correct_chars count, overstating
    /// early-second WPM. Normal ~50ms polling makes multi-second gaps unlikely.
    pub fn tick(&mut self, now: Instant) {
        if self.started_at.is_none() {
            return;
        }
        let secs = self.elapsed(now).as_secs();
        while self.sampled_secs < secs {
            self.sampled_secs += 1;
            self.wpm_samples.push(stats::net_wpm(
                self.correct_chars(),
                self.sampled_secs as f64,
            ));
        }
    }

    /// Positions correct at this moment (after corrections).
    pub fn correct_chars(&self) -> usize {
        self.states[..self.cursor]
            .iter()
            .filter(|s| **s == CharState::Correct)
            .count()
    }

    pub fn live_wpm(&self, now: Instant) -> f64 {
        stats::net_wpm(self.correct_chars(), self.elapsed(now).as_secs_f64())
    }

    pub fn live_accuracy(&self) -> f64 {
        stats::accuracy(self.keystrokes.saturating_sub(self.errors), self.keystrokes)
    }

    pub fn target(&self) -> &[char] {
        &self.target
    }

    pub fn states(&self) -> &[CharState] {
        &self.states
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    #[cfg(test)]
    pub fn wpm_samples(&self) -> &[f64] {
        &self.wpm_samples
    }

    pub fn results(&self) -> SessionResult {
        let secs = self.duration.as_secs_f64();
        SessionResult {
            wpm: stats::net_wpm(self.correct_chars(), secs),
            raw_wpm: stats::raw_wpm(self.keystrokes, secs),
            accuracy: self.live_accuracy(),
            errors: self.errors,
            consistency: stats::consistency(&self.wpm_samples),
            chars: self.keystrokes,
        }
    }

    /// Stats computed over elapsed time at `now`, for a session that may not
    /// have run to completion (e.g. cancelled with Esc). For a finished session
    /// this equals `results()` because `elapsed` is clamped to `duration`.
    /// Zero-safe: before the first keystroke, elapsed is zero and WPM is 0.0.
    pub fn results_at(&self, now: Instant) -> SessionResult {
        let secs = self.elapsed(now).as_secs_f64();
        SessionResult {
            wpm: stats::net_wpm(self.correct_chars(), secs),
            raw_wpm: stats::raw_wpm(self.keystrokes, secs),
            accuracy: self.live_accuracy(),
            errors: self.errors,
            consistency: stats::consistency(&self.wpm_samples),
            chars: self.keystrokes,
        }
    }

    /// Snapshot the mistakes recorded so far this run.
    #[allow(dead_code)]
    pub fn tally(&self) -> MistakeTally {
        let mut words: BTreeMap<String, (u32, u32)> = BTreeMap::new();
        for (start, end, text) in &self.word_spans {
            if *start < self.cursor {
                let upto = (*end).min(self.cursor);
                let fumbled = (*start..upto).any(|p| self.ever_wrong[p]);
                let entry = words.entry(text.clone()).or_insert((0, 0));
                entry.0 += 1;
                if fumbled {
                    entry.1 += 1;
                }
            }
        }
        MistakeTally {
            keys: self.key_tally.clone(),
            words,
            combos: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn new_session() -> Session {
        Session::new(1, Duration::from_secs(30))
    }

    #[test]
    fn target_is_pregenerated_with_lookahead() {
        let s = new_session();
        assert!(s.target().len() >= 120);
        assert!(s
            .target()
            .iter()
            .all(|c| c.is_ascii_lowercase() || *c == ' '));
    }

    #[test]
    fn correct_keystrokes_mark_correct_and_advance() {
        let mut s = new_session();
        let t0 = Instant::now();
        let first: Vec<char> = s.target()[..3].to_vec();
        for &c in &first {
            s.keystroke(c, t0);
        }
        assert_eq!(s.cursor(), 3);
        assert!(s.states()[..3].iter().all(|st| *st == CharState::Correct));
        assert_eq!(s.correct_chars(), 3);
    }

    #[test]
    fn wrong_keystroke_marks_wrong_and_counts_error() {
        let mut s = new_session();
        let t0 = Instant::now();
        s.keystroke('@', t0); // '@' never appears in targets
        assert_eq!(s.states()[0], CharState::Wrong);
        assert_eq!(s.cursor(), 1);
        assert_eq!(s.results().errors, 1);
        assert_eq!(s.correct_chars(), 0);
    }

    #[test]
    fn backspace_allows_correction_but_error_count_stays() {
        let mut s = new_session();
        let t0 = Instant::now();
        let expected = s.target()[0];
        s.keystroke('@', t0);
        s.backspace();
        assert_eq!(s.cursor(), 0);
        assert_eq!(s.states()[0], CharState::Untyped);
        s.keystroke(expected, t0);
        assert_eq!(s.states()[0], CharState::Correct);
        assert_eq!(s.correct_chars(), 1);
        assert_eq!(s.results().errors, 1); // error is not forgiven
        assert_eq!(s.results().chars, 2); // both keystrokes counted
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let mut s = new_session();
        s.backspace();
        assert_eq!(s.cursor(), 0);
    }

    #[test]
    fn timer_starts_on_first_keystroke() {
        let mut s = new_session();
        let t0 = Instant::now();
        // No keystroke yet: never finished, no elapsed time.
        assert!(!s.is_finished(t0 + Duration::from_secs(999)));
        assert_eq!(s.elapsed(t0 + Duration::from_secs(999)), Duration::ZERO);
        let c = s.target()[0];
        s.keystroke(c, t0);
        assert!(!s.is_finished(t0 + Duration::from_secs(29)));
        assert!(s.is_finished(t0 + Duration::from_secs(30)));
    }

    #[test]
    fn elapsed_is_clamped_to_duration() {
        let mut s = new_session();
        let t0 = Instant::now();
        let c = s.target()[0];
        s.keystroke(c, t0);
        assert_eq!(
            s.elapsed(t0 + Duration::from_secs(999)),
            Duration::from_secs(30)
        );
        assert_eq!(s.remaining(t0 + Duration::from_secs(999)), Duration::ZERO);
    }

    #[test]
    fn tick_collects_one_wpm_sample_per_second() {
        let mut s = new_session();
        let t0 = Instant::now();
        let c = s.target()[0];
        s.keystroke(c, t0);
        s.tick(t0 + Duration::from_millis(500));
        assert_eq!(s.wpm_samples().len(), 0);
        s.tick(t0 + Duration::from_secs(3));
        assert_eq!(s.wpm_samples().len(), 3);
        s.tick(t0 + Duration::from_secs(3)); // same second: no new sample
        assert_eq!(s.wpm_samples().len(), 3);
    }

    #[test]
    fn target_extends_as_user_types() {
        let mut s = new_session();
        let t0 = Instant::now();
        for _ in 0..500 {
            let c = s.target()[s.cursor()];
            s.keystroke(c, t0);
        }
        assert_eq!(s.cursor(), 500);
        assert!(s.target().len() >= s.cursor() + 100);
    }

    #[test]
    fn results_compute_all_stats() {
        let mut s = new_session();
        let t0 = Instant::now();
        for _ in 0..50 {
            let c = s.target()[s.cursor()];
            s.keystroke(c, t0);
        }
        s.keystroke('@', t0);
        let r = s.results();
        // 50 correct chars over the full 30s -> (50/5)/(0.5min) = 20 wpm
        assert!((r.wpm - 20.0).abs() < 1e-9);
        assert!((r.raw_wpm - 20.4).abs() < 1e-9); // 51 keystrokes
        assert!((r.accuracy - (50.0 / 51.0 * 100.0)).abs() < 1e-9);
        assert_eq!(r.errors, 1);
        assert_eq!(r.chars, 51);
    }

    #[test]
    fn results_at_uses_elapsed_time() {
        let mut s = new_session(); // 30s duration
        let t0 = Instant::now();
        for _ in 0..50 {
            let c = s.target()[s.cursor()];
            s.keystroke(c, t0);
        }
        // 50 correct chars over 15s elapsed -> (50/5)/(15/60) = 40 wpm
        let r = s.results_at(t0 + Duration::from_secs(15));
        assert!((r.wpm - 40.0).abs() < 1e-9);
        assert_eq!(r.chars, 50);
    }

    #[test]
    fn results_at_before_any_keystroke_is_zero_safe() {
        let s = new_session();
        let t0 = Instant::now();
        let r = s.results_at(t0 + Duration::from_secs(5));
        assert_eq!(r.wpm, 0.0);
        assert_eq!(r.raw_wpm, 0.0);
        assert_eq!(r.chars, 0);
    }

    #[test]
    fn keystroke_after_finish_is_ignored() {
        let mut s = new_session();
        let t0 = Instant::now();
        let c = s.target()[0];
        s.keystroke(c, t0);
        let late = t0 + Duration::from_secs(31);
        s.keystroke('@', late);
        assert_eq!(s.cursor(), 1);
        assert_eq!(s.results().errors, 0);
        assert_eq!(s.results().chars, 1);
    }

    #[test]
    fn tally_counts_key_occurrences_and_wrong() {
        let mut s = new_session();
        let t0 = Instant::now();
        let c0 = s.target()[0]; // a real expected char (never a space at pos 0)
        s.keystroke(c0, t0); // correct
                             // retype position 1 wrong then correct (keystroke-based: 2 occurrences)
        let c1 = s.target()[1];
        s.keystroke('@', t0); // wrong at pos 1
        s.backspace();
        s.keystroke(c1, t0); // correct at pos 1
        let tally = s.tally();
        // c0: 1 occurrence, 0 wrong
        assert_eq!(tally.keys.get(&c0), Some(&(1, 0)));
        // c1: 2 occurrences (wrong then correct), 1 wrong
        assert_eq!(tally.keys.get(&c1), Some(&(2, 1)));
    }

    #[test]
    fn tally_skips_spaces_as_keys() {
        let mut s = new_session();
        let t0 = Instant::now();
        // Type through the first word and its trailing space.
        for _ in 0..10 {
            let c = s.target()[s.cursor()];
            s.keystroke(c, t0);
        }
        assert!(!s.tally().keys.contains_key(&' '));
    }

    #[test]
    fn tally_marks_a_word_fumbled_on_any_wrong_keystroke() {
        let mut s = new_session();
        let t0 = Instant::now();
        // The first word spans target[0..first_space].
        let first_space = s.target().iter().position(|&c| c == ' ').unwrap();
        let word: String = s.target()[..first_space].iter().collect();
        // Fumble the very first character, correct it, then type the rest right.
        s.keystroke('@', t0);
        s.backspace();
        for i in 0..first_space {
            let c = s.target()[i];
            s.keystroke(c, t0);
        }
        // Advance past the space into the next word so the first word is "seen".
        let sp = s.target()[first_space];
        s.keystroke(sp, t0);
        let tally = s.tally();
        assert_eq!(tally.words.get(&word), Some(&(1, 1))); // seen once, fumbled
    }
}
