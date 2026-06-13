use crate::mistakes::WeaknessProfile;
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

/// A blended practice stream: the user's fumbled words plus level-pool words
/// rich in their weak keys/combos. `next_word` flips a coin between the two
/// lists (~50/50), falling back to whichever is non-empty.
pub struct PracticePool {
    fumbled: Vec<String>,
    weak_rich: Vec<String>,
    rng: StdRng,
}

impl PracticePool {
    /// `None` when neither list can be populated (no usable weaknesses) — the
    /// caller should fall back to a normal pool.
    pub fn build(profile: &WeaknessProfile, level: u8) -> Option<Self> {
        Self::with_rng(profile, level, StdRng::from_entropy())
    }

    #[cfg(test)]
    pub fn seeded(profile: &WeaknessProfile, level: u8, seed: u64) -> Option<Self> {
        Self::with_rng(profile, level, StdRng::seed_from_u64(seed))
    }

    fn with_rng(profile: &WeaknessProfile, level: u8, rng: StdRng) -> Option<Self> {
        let fumbled: Vec<String> = profile.words.iter().map(|w| w.label.clone()).collect();
        // Weak single keys match by character; weak combos match by substring
        // (a "th" weakness should pull words containing "th", not every word
        // with a 't' or an 'h').
        let weak_keys: Vec<char> = profile
            .keys
            .iter()
            .flat_map(|w| w.label.chars())
            .filter(|c| c.is_ascii_lowercase())
            .collect();
        let weak_combos: Vec<&str> = profile.combos.iter().map(|w| w.label.as_str()).collect();
        let fumbled_set: std::collections::HashSet<&str> =
            fumbled.iter().map(String::as_str).collect();
        let mut weak_rich: Vec<String> = pool_words(level)
            .into_iter()
            .filter(|w| {
                !fumbled_set.contains(w)
                    && (weak_keys.iter().any(|c| w.contains(*c))
                        || weak_combos.iter().any(|s| w.contains(s)))
            })
            .map(|w| w.to_string())
            .collect();
        if fumbled.is_empty() && weak_rich.is_empty() {
            return None;
        }
        // Guarantee variety: a profile with, say, one fumbled word and no
        // weak-key matches would otherwise repeat that single word for the
        // whole session. Pad weak_rich with normal level words (deduped) so the
        // stream stays varied while still drilling the real weaknesses.
        const MIN_VARIETY: usize = 8;
        if fumbled.len() + weak_rich.len() < MIN_VARIETY {
            let mut have: std::collections::HashSet<String> =
                fumbled.iter().chain(weak_rich.iter()).cloned().collect();
            for w in pool_words(level) {
                if fumbled.len() + weak_rich.len() >= MIN_VARIETY {
                    break;
                }
                if have.insert(w.to_string()) {
                    weak_rich.push(w.to_string());
                }
            }
        }
        Some(PracticePool {
            fumbled,
            weak_rich,
            rng,
        })
    }
}

impl WordStream for PracticePool {
    fn next_word(&mut self, prev: Option<&str>) -> String {
        let pick = |rng: &mut StdRng, list: &[String]| list[rng.gen_range(0..list.len())].clone();
        for _ in 0..8 {
            let from_fumbled = if self.fumbled.is_empty() {
                false
            } else if self.weak_rich.is_empty() {
                true
            } else {
                self.rng.gen_bool(0.5)
            };
            let w = if from_fumbled {
                pick(&mut self.rng, &self.fumbled)
            } else {
                pick(&mut self.rng, &self.weak_rich)
            };
            if Some(w.as_str()) != prev {
                return w;
            }
        }
        // Tiny pool: accept a repeat rather than loop forever.
        // Prefer weak_rich in the fallback: it is almost always larger than
        // fumbled, giving better variety under repeat pressure.
        if !self.weak_rich.is_empty() {
            pick(&mut self.rng, &self.weak_rich)
        } else {
            pick(&mut self.rng, &self.fumbled)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mistakes::{Weakness, WeaknessProfile};

    fn profile_with(keys: &[&str], words: &[&str]) -> WeaknessProfile {
        let mk = |label: &str| Weakness {
            label: label.to_string(),
            total: 10,
            bad: 5,
            rate: 0.5,
        };
        WeaknessProfile {
            keys: keys.iter().map(|k| mk(k)).collect(),
            combos: Vec::new(),
            words: words.iter().map(|w| mk(w)).collect(),
            sessions: 5,
        }
    }

    #[test]
    fn practice_pool_includes_fumbled_words() {
        let profile = profile_with(&["b"], &["because", "weird"]);
        let mut pool = PracticePool::seeded(&profile, 3, 1).unwrap();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..400 {
            seen.insert(pool.next_word(None));
        }
        assert!(seen.contains("because") || seen.contains("weird"));
        // weak-key-rich words containing 'b' also appear
        assert!(seen.iter().any(|w| w.contains('b')));
    }

    #[test]
    fn practice_pool_is_none_without_signal() {
        // No fumbled words and a weak "key" that is not an ascii letter -> no
        // candidate words can be built.
        let profile = profile_with(&["!"], &[]);
        assert!(PracticePool::seeded(&profile, 3, 1).is_none());
    }

    #[test]
    fn practice_pool_seeded_is_deterministic() {
        let profile = profile_with(&["e"], &["the"]);
        let mut a = PracticePool::seeded(&profile, 2, 9).unwrap();
        let mut b = PracticePool::seeded(&profile, 2, 9).unwrap();
        for _ in 0..20 {
            assert_eq!(a.next_word(None), b.next_word(None));
        }
    }

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
            for w in pool_words(level) {
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
            for w in pool_words(level) {
                assert!(w.len() >= 2, "single-letter word at level {level}: {w:?}");
            }
        }
    }

    #[test]
    fn level_one_words_are_short() {
        for w in pool_words(1) {
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

    #[test]
    fn practice_pool_single_fumbled_word_still_varies() {
        // One fumbled word, no weak keys/combos -> must NOT repeat one word.
        let profile = profile_with(&[], &["because"]);
        let mut pool = PracticePool::seeded(&profile, 1, 5).unwrap();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..50 {
            seen.insert(pool.next_word(None));
        }
        assert!(
            seen.len() >= 2,
            "practice pool should offer variety, got {seen:?}"
        );
    }

    #[test]
    fn practice_weak_combo_matches_substring_not_chars() {
        // A combo weakness "qu": every practice word must contain the bigram
        // "qu" as a substring — not merely a 'q' or a 'u'.
        let profile = WeaknessProfile {
            keys: Vec::new(),
            combos: vec![Weakness {
                label: "qu".into(),
                total: 10,
                bad: 5,
                rate: 0.5,
            }],
            words: Vec::new(),
            sessions: 5,
        };
        let mut pool = PracticePool::seeded(&profile, 5, 3).unwrap();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..400 {
            seen.insert(pool.next_word(None));
        }
        assert!(!seen.is_empty());
        assert!(seen.iter().all(|w| w.contains("qu")));
    }
}
