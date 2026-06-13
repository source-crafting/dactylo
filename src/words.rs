use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

const RAW_WORDS: &str = include_str!("../assets/words-en.txt");

fn all_words() -> impl Iterator<Item = &'static str> {
    RAW_WORDS
        .lines()
        .map(str::trim)
        .filter(|w| w.len() >= 2 && w.chars().all(|c| c.is_ascii_lowercase()))
}

/// The candidate words for a level (frequency-ranked slices of the full list).
fn pool_words(level: u8) -> Vec<&'static str> {
    match level {
        1 => all_words().filter(|w| w.len() <= 5).take(200).collect(),
        2 => all_words().take(1000).collect(),
        3 => all_words().take(3000).collect(),
        4 => all_words().take(7000).collect(),
        _ => all_words().collect(),
    }
}

/// A stream of words for a session. `next_word` avoids immediately repeating
/// `prev` when the pool allows it. Owning the RNG lets `Session` stay agnostic
/// to whether it is running a normal or a practice stream.
pub trait WordStream {
    fn next_word(&mut self, prev: Option<&str>) -> String;
}

pub struct WordPool {
    words: Vec<&'static str>,
    rng: StdRng,
}

impl WordPool {
    /// Levels 1-5; any value outside 1-4 returns the full list.
    pub fn for_level(level: u8) -> Self {
        WordPool {
            words: pool_words(level),
            rng: StdRng::from_entropy(),
        }
    }

    /// Deterministic pool for tests.
    #[cfg(test)]
    pub fn seeded(level: u8, seed: u64) -> Self {
        WordPool {
            words: pool_words(level),
            rng: StdRng::seed_from_u64(seed),
        }
    }

    /// The candidate words (used by the practice pool to find weak-key-rich words).
    #[allow(dead_code)]
    pub fn words(&self) -> &[&'static str] {
        &self.words
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.words.len()
    }
}

impl WordStream for WordPool {
    fn next_word(&mut self, prev: Option<&str>) -> String {
        assert!(!self.words.is_empty(), "WordPool is empty");
        loop {
            let w = self.words[self.rng.gen_range(0..self.words.len())];
            if Some(w) != prev || self.words.len() == 1 {
                return w.to_string();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_sizes_match_levels() {
        assert_eq!(WordPool::for_level(1).len(), 200);
        assert_eq!(WordPool::for_level(2).len(), 1000);
        assert_eq!(WordPool::for_level(3).len(), 3000);
        assert_eq!(WordPool::for_level(4).len(), 7000);
        assert!(WordPool::for_level(5).len() > 7000);
    }

    #[test]
    fn all_words_are_lowercase_ascii() {
        for level in 1..=5 {
            for w in WordPool::for_level(level).words() {
                assert!(
                    !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase()),
                    "bad word at level {level}: {w:?}"
                );
            }
        }
    }

    #[test]
    fn excludes_single_letter_words() {
        for level in 1..=5 {
            for w in WordPool::for_level(level).words() {
                assert!(w.len() >= 2, "single-letter word at level {level}: {w:?}");
            }
        }
    }

    #[test]
    fn level_one_words_are_short() {
        for w in WordPool::for_level(1).words() {
            assert!(w.len() <= 5, "too long for level 1: {w}");
        }
    }

    #[test]
    fn no_immediate_repetition() {
        let mut pool = WordPool::seeded(1, 42);
        let mut prev = pool.next_word(None);
        for _ in 0..1000 {
            let next = pool.next_word(Some(&prev));
            assert_ne!(next, prev);
            prev = next;
        }
    }

    #[test]
    fn seeded_pool_is_deterministic() {
        let mut a = WordPool::seeded(2, 7);
        let mut b = WordPool::seeded(2, 7);
        for _ in 0..20 {
            assert_eq!(a.next_word(None), b.next_word(None));
        }
    }
}
