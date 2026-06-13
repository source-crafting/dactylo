use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Append one record as a JSON line to `path`, creating parent dirs as needed.
pub fn append<T: Serialize>(path: &Path, record: &T) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    let json = serde_json::to_string(record)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    writeln!(f, "{json}")
}

/// Read `path` once, returning all records that parse AND pass `is_plausible`,
/// plus the count of non-empty lines that were unusable (unparseable or
/// implausible). A missing/unreadable file yields `(vec![], 0)`.
pub fn load_with_skipped<T, F>(path: &Path, is_plausible: F) -> (Vec<T>, usize)
where
    T: DeserializeOwned,
    F: Fn(&T) -> bool,
{
    let Ok(content) = fs::read_to_string(path) else {
        return (Vec::new(), 0);
    };
    let mut records = Vec::new();
    let mut skipped = 0;
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<T>(line) {
            Ok(r) if is_plausible(&r) => records.push(r),
            _ => skipped += 1,
        }
    }
    (records, skipped)
}
