use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
    /// within-word two-char transition "ab" -> (occurrences, wrong)
    pub combos: BTreeMap<String, (u32, u32)>,
}

impl MistakeTally {
    /// True when nothing was typed worth recording (no keystrokes landed).
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.words.is_empty() && self.combos.is_empty()
    }
}

/// Recent-window size: weaknesses reflect roughly the last N sessions.
pub const RECENT_WINDOW: usize = 20;
/// Minimum samples before an item can be ranked (avoids "100% on 1 try").
pub const MIN_KEY_OCC: u32 = 10;
/// Bigrams recur less than single keys, so they rank at a slightly lower bar.
pub const MIN_COMBO_OCC: u32 = 8;
/// A given word is seen only a handful of times per session, so the bar is low.
pub const MIN_WORD_SEEN: u32 = 3;

/// One session's mistakes, persisted as a line of `mistakes.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MistakeRecord {
    pub ts: String,
    pub practice: bool,
    pub level: u8,
    pub keys: BTreeMap<char, (u32, u32)>,
    #[serde(default)]
    pub words: BTreeMap<String, (u32, u32)>,
    #[serde(default)]
    pub combos: BTreeMap<String, (u32, u32)>,
}

impl MistakeRecord {
    /// Build a record from a tally, stamping the timestamp and mode.
    pub fn from_tally(tally: &MistakeTally, level: u8, practice: bool, ts: String) -> Self {
        MistakeRecord {
            ts,
            practice,
            level,
            keys: tally.keys.clone(),
            words: tally.words.clone(),
            combos: tally.combos.clone(),
        }
    }

    /// Reject corrupt/hand-edited lines: bad counts (wrong > total) or an
    /// impossible level can't poison the profile.
    fn is_plausible(&self) -> bool {
        (1..=5).contains(&self.level)
            && self.keys.values().all(|&(o, w)| w <= o)
            && self.words.values().all(|&(s, f)| f <= s)
            && self.combos.values().all(|&(o, w)| w <= o)
    }
}

/// Append-only log of per-session mistake detail at `~/.dactylo/mistakes.jsonl`.
pub struct MistakeLog {
    path: PathBuf,
}

impl MistakeLog {
    pub fn new(dir: PathBuf) -> Self {
        MistakeLog {
            path: dir.join("mistakes.jsonl"),
        }
    }

    pub fn append(&self, record: &MistakeRecord) -> std::io::Result<()> {
        crate::jsonl::append(&self.path, record)
    }

    /// Read once; return readable records plus the count of unusable
    /// (unparseable or implausible) non-empty lines.
    pub fn load_with_skipped(&self) -> (Vec<MistakeRecord>, usize) {
        crate::jsonl::load_with_skipped(&self.path, |r: &MistakeRecord| r.is_plausible())
    }
}

/// Which order the explorer lists weaknesses in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    /// Highest error rate first.
    Rate,
    /// Most total misses first.
    Count,
}

/// One ranked weakness row.
#[derive(Debug, Clone, PartialEq)]
pub struct Weakness {
    pub label: String,
    pub total: u32,
    pub bad: u32,
    pub rate: f64,
}

/// Ranked weaknesses over a recent window of sessions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WeaknessProfile {
    pub keys: Vec<Weakness>,
    pub combos: Vec<Weakness>,
    pub words: Vec<Weakness>,
    /// How many sessions were aggregated (window size, capped at RECENT_WINDOW).
    pub sessions: usize,
}

/// Sort weakness rows in place. Ties broken by the other metric, then label,
/// so the order is deterministic.
pub fn sort_weaknesses(v: &mut [Weakness], sort: Sort) {
    match sort {
        Sort::Rate => v.sort_by(|a, b| {
            b.rate
                .partial_cmp(&a.rate)
                .unwrap()
                .then(b.bad.cmp(&a.bad))
                .then(a.label.cmp(&b.label))
        }),
        Sort::Count => v.sort_by(|a, b| {
            b.bad
                .cmp(&a.bad)
                .then(b.rate.partial_cmp(&a.rate).unwrap())
                .then(a.label.cmp(&b.label))
        }),
    }
}

fn rank(map: BTreeMap<String, (u32, u32)>, min_total: u32, sort: Sort) -> Vec<Weakness> {
    let mut v: Vec<Weakness> = map
        .into_iter()
        .filter(|(_, (total, bad))| *total >= min_total && *bad > 0)
        .map(|(label, (total, bad))| Weakness {
            label,
            total,
            bad,
            rate: bad as f64 / total as f64,
        })
        .collect();
    sort_weaknesses(&mut v, sort);
    v
}

impl WeaknessProfile {
    /// Aggregate the most recent `RECENT_WINDOW` records into ranked weaknesses.
    pub fn from_records(records: &[MistakeRecord], sort: Sort) -> Self {
        let window = if records.len() > RECENT_WINDOW {
            &records[records.len() - RECENT_WINDOW..]
        } else {
            records
        };
        let mut keys: BTreeMap<String, (u32, u32)> = BTreeMap::new();
        let mut combos: BTreeMap<String, (u32, u32)> = BTreeMap::new();
        let mut words: BTreeMap<String, (u32, u32)> = BTreeMap::new();
        for r in window {
            for (k, &(o, w)) in &r.keys {
                let e = keys.entry(k.to_string()).or_insert((0, 0));
                e.0 += o;
                e.1 += w;
            }
            for (k, &(o, w)) in &r.combos {
                let e = combos.entry(k.clone()).or_insert((0, 0));
                e.0 += o;
                e.1 += w;
            }
            for (k, &(s, f)) in &r.words {
                let e = words.entry(k.clone()).or_insert((0, 0));
                e.0 += s;
                e.1 += f;
            }
        }
        WeaknessProfile {
            keys: rank(keys, MIN_KEY_OCC, sort),
            combos: rank(combos, MIN_COMBO_OCC, sort),
            words: rank(words, MIN_WORD_SEEN, sort),
            sessions: window.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.combos.is_empty() && self.words.is_empty()
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

    fn rec(level: u8, keys: &[(char, u32, u32)], words: &[(&str, u32, u32)]) -> MistakeRecord {
        MistakeRecord {
            ts: "2026-06-13T00:00:00Z".into(),
            practice: false,
            level,
            keys: keys.iter().map(|&(c, o, w)| (c, (o, w))).collect(),
            words: words
                .iter()
                .map(|&(s, o, w)| (s.to_string(), (o, w)))
                .collect(),
            combos: BTreeMap::new(),
        }
    }

    #[test]
    fn append_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let log = MistakeLog::new(dir.path().to_path_buf());
        log.append(&rec(3, &[('b', 10, 4)], &[("because", 3, 2)]))
            .unwrap();
        let (recs, skipped) = log.load_with_skipped();
        assert_eq!(skipped, 0);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].keys.get(&'b'), Some(&(10, 4)));
        assert_eq!(recs[0].words.get("because"), Some(&(3, 2)));
    }

    #[test]
    fn missing_words_and_combos_default_to_empty() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let log = MistakeLog::new(dir.path().to_path_buf());
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.path().join("mistakes.jsonl"))
            .unwrap();
        writeln!(
            f,
            r#"{{"ts":"x","practice":false,"level":3,"keys":{{"b":[10,4]}}}}"#
        )
        .unwrap();
        let (recs, skipped) = log.load_with_skipped();
        assert_eq!(skipped, 0);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].words.is_empty());
        assert!(recs[0].combos.is_empty());
    }

    #[test]
    fn implausible_lines_are_skipped() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let log = MistakeLog::new(dir.path().to_path_buf());
        log.append(&rec(3, &[('b', 10, 4)], &[])).unwrap();
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.path().join("mistakes.jsonl"))
            .unwrap();
        writeln!(f, "not json").unwrap();
        // wrong > occurrences is corrupt:
        writeln!(
            f,
            r#"{{"ts":"x","practice":false,"level":3,"keys":{{"b":[2,9]}},"words":{{}},"combos":{{}}}}"#
        )
        .unwrap();
        let (recs, skipped) = log.load_with_skipped();
        assert_eq!(recs.len(), 1);
        assert_eq!(skipped, 2);
    }

    #[test]
    fn profile_aggregates_ranks_and_thresholds() {
        // 'b': 12 occ / 6 wrong = 50%; 'p': 80 occ / 8 wrong = 10%.
        // 'z': only 4 occ -> below MIN_KEY_OCC, excluded.
        let recs = vec![
            rec(
                3,
                &[('b', 6, 3), ('p', 40, 4), ('z', 2, 2)],
                &[("the", 2, 0)],
            ),
            rec(
                3,
                &[('b', 6, 3), ('p', 40, 4), ('z', 2, 2)],
                &[("weird", 3, 2)],
            ),
        ];
        let p = WeaknessProfile::from_records(&recs, Sort::Rate);
        assert_eq!(p.sessions, 2);
        let labels: Vec<&str> = p.keys.iter().map(|k| k.label.as_str()).collect();
        assert_eq!(labels, vec!["b", "p"]); // z excluded; b before p by rate
        assert!((p.keys[0].rate - 0.5).abs() < 1e-9);
        // word "the" has 0 fumbles -> excluded; "weird" 3 seen / 2 fumbled kept
        let words: Vec<&str> = p.words.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(words, vec!["weird"]);
    }

    #[test]
    fn profile_uses_only_the_recent_window() {
        // One old 'p'-only record, then RECENT_WINDOW 'b'-only records. The 'p'
        // record is pushed out of the last-N window entirely, so 'p' must vanish.
        let mut recs = vec![rec(3, &[('p', 20, 10)], &[])];
        recs.extend(std::iter::repeat_n(
            rec(3, &[('b', 20, 10)], &[]),
            RECENT_WINDOW,
        ));
        let p = WeaknessProfile::from_records(&recs, Sort::Rate);
        let labels: Vec<&str> = p.keys.iter().map(|k| k.label.as_str()).collect();
        assert!(labels.contains(&"b"));
        assert!(!labels.contains(&"p")); // the only 'p' record fell out of the window
        assert_eq!(p.sessions, RECENT_WINDOW); // window is capped at RECENT_WINDOW
    }

    #[test]
    fn sort_by_count_orders_by_bad_total() {
        // 'a': 100 occ / 9 wrong (9% but 9 misses); 'b': 12 occ / 6 wrong (50%, 6 misses)
        let recs = vec![rec(3, &[('a', 100, 9), ('b', 12, 6)], &[])];
        let p = WeaknessProfile::from_records(&recs, Sort::Count);
        let labels: Vec<&str> = p.keys.iter().map(|k| k.label.as_str()).collect();
        assert_eq!(labels, vec!["a", "b"]); // more total misses first
    }

    #[test]
    fn combos_roundtrip_and_rank() {
        let dir = tempfile::tempdir().unwrap();
        let log = MistakeLog::new(dir.path().to_path_buf());
        let mut r = rec(3, &[], &[]);
        r.combos.insert("io".into(), (10, 4)); // 40%, >= MIN_COMBO_OCC
        r.combos.insert("zz".into(), (3, 3)); // below MIN_COMBO_OCC -> excluded
        log.append(&r).unwrap();
        let (recs, skipped) = log.load_with_skipped();
        assert_eq!(skipped, 0);
        assert_eq!(recs[0].combos.get("io"), Some(&(10, 4)));
        let p = WeaknessProfile::from_records(&recs, Sort::Rate);
        let labels: Vec<&str> = p.combos.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, vec!["io"]);
    }
}
