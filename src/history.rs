use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Record {
    pub ts: String,
    pub duration: u64,
    pub level: u8,
    pub wpm: f64,
    pub raw_wpm: f64,
    pub accuracy: f64,
    pub errors: usize,
    pub consistency: f64,
    pub chars: usize,
}

impl Record {
    /// Whether the numeric fields are in range. Parseable-but-out-of-range lines
    /// (corrupt or hand-edited — e.g. a negative or non-finite WPM, an
    /// impossible level) are treated as skipped on load so a garbage value can't
    /// poison aggregates or overflow the display.
    fn is_plausible(&self) -> bool {
        (1..=5).contains(&self.level)
            && self.wpm.is_finite()
            && self.wpm >= 0.0
            && self.raw_wpm.is_finite()
            && self.raw_wpm >= 0.0
            && (0.0..=100.0).contains(&self.accuracy)
            && (0.0..=100.0).contains(&self.consistency)
    }
}

#[derive(Debug, PartialEq)]
pub struct LevelSummary {
    pub count: usize,
    pub avg_wpm: f64,
    pub best_wpm: f64,
    pub avg_accuracy: f64,
    pub best_accuracy: f64,
    pub best_raw_wpm: f64,
    pub min_errors: usize,
    pub best_consistency: f64,
}

/// One session's chartable metrics, in chronological order within a level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionPoint {
    pub wpm: f64,
    pub accuracy: f64,
}

pub struct History {
    path: PathBuf,
}

/// Aggregate the records for `level` into a `LevelSummary`, or `None` if there
/// are no sessions at that level.
pub fn summary_of(records: &[Record], level: u8) -> Option<LevelSummary> {
    let recs: Vec<&Record> = records.iter().filter(|r| r.level == level).collect();
    if recs.is_empty() {
        return None;
    }
    let n = recs.len() as f64;
    Some(LevelSummary {
        count: recs.len(),
        avg_wpm: recs.iter().map(|r| r.wpm).sum::<f64>() / n,
        best_wpm: recs.iter().map(|r| r.wpm).fold(f64::MIN, f64::max),
        avg_accuracy: recs.iter().map(|r| r.accuracy).sum::<f64>() / n,
        best_accuracy: recs.iter().map(|r| r.accuracy).fold(f64::MIN, f64::max),
        best_raw_wpm: recs.iter().map(|r| r.raw_wpm).fold(f64::MIN, f64::max),
        min_errors: recs.iter().map(|r| r.errors).min().unwrap_or(0),
        best_consistency: recs.iter().map(|r| r.consistency).fold(f64::MIN, f64::max),
    })
}

/// The chartable (wpm, accuracy) series for `level`, in file (chronological)
/// order.
pub fn series_of(records: &[Record], level: u8) -> Vec<SessionPoint> {
    records
        .iter()
        .filter(|r| r.level == level)
        .map(|r| SessionPoint {
            wpm: r.wpm,
            accuracy: r.accuracy,
        })
        .collect()
}

impl History {
    /// `~/.dactylo`, or None if the home directory cannot be determined.
    pub fn default_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".dactylo"))
    }

    pub fn new(dir: PathBuf) -> Self {
        History {
            path: dir.join("history.jsonl"),
        }
    }

    pub fn append(&self, record: &Record) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let json = serde_json::to_string(record)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        writeln!(f, "{json}")
    }

    /// Read the history file once, returning all readable records plus the count
    /// of non-empty lines that were unusable — either unparseable or parseable
    /// but out of range (see [`Record::is_plausible`]).
    pub fn load_with_skipped(&self) -> (Vec<Record>, usize) {
        let Ok(content) = fs::read_to_string(&self.path) else {
            return (Vec::new(), 0);
        };
        let mut records = Vec::new();
        let mut skipped = 0;
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Record>(line) {
                Ok(r) if r.is_plausible() => records.push(r),
                _ => skipped += 1,
            }
        }
        (records, skipped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn record(level: u8, wpm: f64, accuracy: f64) -> Record {
        Record {
            ts: "2026-06-10T00:00:00Z".into(),
            duration: 60,
            level,
            wpm,
            raw_wpm: wpm + 2.0,
            accuracy,
            errors: 3,
            consistency: 90.0,
            chars: 250,
        }
    }

    #[test]
    fn append_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let h = History::new(dir.path().to_path_buf());
        h.append(&record(3, 50.0, 95.0)).unwrap();
        h.append(&record(3, 60.0, 97.0)).unwrap();
        let recs = h.load_with_skipped().0;
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[1].wpm, 60.0);
        assert_eq!(recs[0].level, 3);
    }

    #[test]
    fn append_creates_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("does-not-exist-yet");
        let h = History::new(nested);
        h.append(&record(1, 40.0, 90.0)).unwrap();
        assert_eq!(h.load_with_skipped().0.len(), 1);
    }

    #[test]
    fn load_returns_empty_when_no_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(History::new(dir.path().to_path_buf())
            .load_with_skipped()
            .0
            .is_empty());
    }

    #[test]
    fn summary_of_filters_by_level_and_aggregates() {
        // record(level, wpm, accuracy) sets raw_wpm = wpm + 2.0, errors = 3,
        // consistency = 90.0 (see the `record` helper).
        let recs = vec![
            record(3, 50.0, 95.0),
            record(3, 60.0, 97.0),
            record(2, 99.0, 99.0),
        ];
        let s = super::summary_of(&recs, 3).unwrap();
        assert_eq!(s.count, 2);
        assert!((s.avg_wpm - 55.0).abs() < 1e-9);
        assert_eq!(s.best_wpm, 60.0);
        assert!((s.avg_accuracy - 96.0).abs() < 1e-9);
        assert_eq!(s.best_accuracy, 97.0);
        assert_eq!(s.best_raw_wpm, 62.0); // max(52, 62)
        assert_eq!(s.min_errors, 3); // both records have errors = 3
        assert_eq!(s.best_consistency, 90.0); // both have consistency = 90
    }

    #[test]
    fn summary_of_empty_level_is_none() {
        let recs = vec![record(1, 40.0, 90.0)];
        assert!(super::summary_of(&recs, 5).is_none());
    }

    #[test]
    fn series_of_keeps_order_and_filters_level() {
        let recs = vec![
            record(3, 50.0, 95.0),
            record(2, 10.0, 80.0),
            record(3, 60.0, 97.0),
        ];
        let series = super::series_of(&recs, 3);
        assert_eq!(series.len(), 2);
        assert_eq!(series[0].wpm, 50.0);
        assert_eq!(series[1].wpm, 60.0);
        assert_eq!(series[1].accuracy, 97.0);
    }

    #[test]
    fn series_of_empty_when_no_sessions() {
        assert!(super::series_of(&[], 3).is_empty());
    }

    #[test]
    fn load_with_skipped_returns_records_and_corrupt_count() {
        let dir = tempfile::tempdir().unwrap();
        let h = History::new(dir.path().to_path_buf());
        h.append(&record(3, 50.0, 95.0)).unwrap();
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.path().join("history.jsonl"))
            .unwrap();
        use std::io::Write;
        writeln!(f, "not json").unwrap();
        h.append(&record(3, 60.0, 97.0)).unwrap();
        let (records, skipped) = h.load_with_skipped();
        assert_eq!(records.len(), 2);
        assert_eq!(skipped, 1);
    }

    #[test]
    fn load_with_skipped_empty_when_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let (records, skipped) = History::new(dir.path().to_path_buf()).load_with_skipped();
        assert!(records.is_empty());
        assert_eq!(skipped, 0);
    }

    #[test]
    fn implausible_records_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let h = History::new(dir.path().to_path_buf());
        h.append(&record(3, 50.0, 95.0)).unwrap();
        // Parseable JSON but out-of-range values (corrupt / hand-edited).
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.path().join("history.jsonl"))
            .unwrap();
        writeln!(
            f,
            r#"{{"ts":"x","duration":60,"level":3,"wpm":-1e308,"raw_wpm":0,"accuracy":50,"errors":0,"consistency":50,"chars":0}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"ts":"x","duration":60,"level":9,"wpm":50,"raw_wpm":50,"accuracy":150,"errors":0,"consistency":50,"chars":0}}"#
        )
        .unwrap();
        let (records, skipped) = h.load_with_skipped();
        assert_eq!(records.len(), 1); // only the valid record survives
        assert_eq!(skipped, 2); // both out-of-range lines counted as corrupt
    }
}
