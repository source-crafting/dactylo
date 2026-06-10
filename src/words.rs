use rand::Rng;

const RAW_WORDS: &str = include_str!("../assets/words-en.txt");

fn all_words() -> impl Iterator<Item = &'static str> {
    RAW_WORDS
        .lines()
        .map(str::trim)
        .filter(|w| w.len() >= 2 && w.chars().all(|c| c.is_ascii_lowercase()))
}

pub struct WordPool {
    words: Vec<&'static str>,
}

impl WordPool {
    /// Levels 1-5; any value outside 1-4 returns the full list.
    pub fn for_level(level: u8) -> Self {
        let words: Vec<&'static str> = match level {
            1 => all_words().filter(|w| w.len() <= 5).take(200).collect(),
            2 => all_words().take(1000).collect(),
            3 => all_words().take(3000).collect(),
            4 => all_words().take(7000).collect(),
            _ => all_words().collect(),
        };
        WordPool { words }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.words.len()
    }

    #[cfg(test)]
    pub fn words(&self) -> &[&'static str] {
        &self.words
    }

    /// Uniform random word, never equal to `prev` (unless the pool has one word).
    pub fn next_word(&self, prev: Option<&str>, rng: &mut impl Rng) -> &'static str {
        assert!(!self.words.is_empty(), "WordPool is empty");
        loop {
            let w = self.words[rng.gen_range(0..self.words.len())];
            if Some(w) != prev || self.words.len() == 1 {
                return w;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

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
        let pool = WordPool::for_level(1);
        let mut rng = StdRng::seed_from_u64(42);
        let mut prev = pool.next_word(None, &mut rng);
        for _ in 0..1000 {
            let next = pool.next_word(Some(prev), &mut rng);
            assert_ne!(next, prev);
            prev = next;
        }
    }
}
