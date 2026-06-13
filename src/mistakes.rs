use std::collections::BTreeMap;

/// Per-session mistake accumulation. Key/combo counts are keystroke-based
/// (matching how `accuracy` counts every keystroke); word counts are
/// per-instance: `seen` = times the word was reached, `fumbled` = instances
/// with at least one wrong keystroke inside.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MistakeTally {
    /// expected char -> (occurrences, wrong)
    pub keys: BTreeMap<char, (u32, u32)>,
    /// word -> (seen, fumbled)
    pub words: BTreeMap<String, (u32, u32)>,
    /// two-char transition "ab" -> (occurrences, wrong). Populated in Phase 2.
    pub combos: BTreeMap<String, (u32, u32)>,
}

impl MistakeTally {
    /// True when nothing was typed worth recording (no keystrokes landed).
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.words.is_empty() && self.combos.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tally_is_empty() {
        assert!(MistakeTally::default().is_empty());
    }

    #[test]
    fn non_empty_when_a_key_is_recorded() {
        let mut t = MistakeTally::default();
        t.keys.insert('b', (3, 1));
        assert!(!t.is_empty());
    }

    #[test]
    fn non_empty_when_a_word_is_recorded() {
        let mut t = MistakeTally::default();
        t.words.insert("because".to_string(), (3, 2));
        assert!(!t.is_empty());
    }

    #[test]
    fn non_empty_when_a_combo_is_recorded() {
        let mut t = MistakeTally::default();
        t.combos.insert("io".to_string(), (10, 4));
        assert!(!t.is_empty());
    }
}
