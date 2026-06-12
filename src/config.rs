use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde::{Deserialize, Serialize};

/// Durations offered by the interactive config screen, in seconds.
pub const DURATIONS: [u64; 4] = [15, 30, 60, 120];

#[derive(Parser, Debug)]
#[command(name = "dactylo", about = "Terminal typing trainer", version)]
pub struct Cli {
    /// Session duration in seconds (5-600)
    #[arg(long)]
    pub time: Option<u64>,

    /// Difficulty level (1-5)
    #[arg(long)]
    pub level: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub duration_secs: u64,
    pub level: u8,
}

impl Settings {
    /// `Ok(None)` means no flags were given: show the interactive config screen.
    pub fn from_cli(cli: &Cli) -> Result<Option<Settings>, String> {
        if cli.time.is_none() && cli.level.is_none() {
            return Ok(None);
        }
        let duration_secs = cli.time.unwrap_or(60);
        let level = cli.level.unwrap_or(3);
        if !(5..=600).contains(&duration_secs) {
            return Err("--time must be between 5 and 600 seconds".into());
        }
        if !(1..=5).contains(&level) {
            return Err("--level must be between 1 and 5".into());
        }
        Ok(Some(Settings {
            duration_secs,
            level,
        }))
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            duration_secs: 60,
            level: 3,
        }
    }
}

impl Settings {
    /// `~/.dactylo/settings.json`, or `None` if the home directory is unknown.
    pub fn default_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".dactylo").join("settings.json"))
    }

    /// Read settings from `path`, returning `None` if the file is missing or
    /// cannot be parsed. Callers use this to tell "no saved settings" apart from
    /// "saved settings happen to equal the defaults".
    pub fn try_load_from(path: &Path) -> Option<Settings> {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    /// Whether both fields are within the accepted ranges (same bounds the CLI
    /// enforces): duration 5..=600s, level 1..=5.
    pub fn is_valid(&self) -> bool {
        (5..=600).contains(&self.duration_secs) && (1..=5).contains(&self.level)
    }

    /// Load settings from the default path, returning `None` when no readable,
    /// in-range settings file exists yet (e.g. first run, or a corrupt /
    /// hand-edited file with out-of-range values). Used to decide whether to
    /// launch straight into a session or show the setup screen.
    pub fn load_existing() -> Option<Settings> {
        Self::try_load_from(&Self::default_path()?).filter(Settings::is_valid)
    }

    /// Best-effort write of settings to `path` (creates parent directories).
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(path, json)
    }

    /// Best-effort save to the default path; errors are ignored so a failed
    /// write never blocks play.
    pub fn save(&self) {
        if let Some(p) = Self::default_path() {
            let _ = self.save_to(&p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_flags_means_interactive() {
        let cli = Cli {
            time: None,
            level: None,
        };
        assert_eq!(Settings::from_cli(&cli).unwrap(), None);
    }

    #[test]
    fn full_flags_produce_settings() {
        let cli = Cli {
            time: Some(120),
            level: Some(5),
        };
        assert_eq!(
            Settings::from_cli(&cli).unwrap(),
            Some(Settings {
                duration_secs: 120,
                level: 5
            })
        );
    }

    #[test]
    fn partial_flags_fill_defaults() {
        let cli = Cli {
            time: Some(30),
            level: None,
        };
        assert_eq!(
            Settings::from_cli(&cli).unwrap(),
            Some(Settings {
                duration_secs: 30,
                level: 3
            })
        );
        let cli = Cli {
            time: None,
            level: Some(1),
        };
        assert_eq!(
            Settings::from_cli(&cli).unwrap(),
            Some(Settings {
                duration_secs: 60,
                level: 1
            })
        );
    }

    #[test]
    fn invalid_values_rejected() {
        assert!(Settings::from_cli(&Cli {
            time: Some(3),
            level: None
        })
        .is_err());
        assert!(Settings::from_cli(&Cli {
            time: Some(601),
            level: None
        })
        .is_err());
        assert!(Settings::from_cli(&Cli {
            time: None,
            level: Some(0)
        })
        .is_err());
        assert!(Settings::from_cli(&Cli {
            time: None,
            level: Some(9)
        })
        .is_err());
    }

    #[test]
    fn settings_default_is_60s_level_3() {
        assert_eq!(
            Settings::default(),
            Settings {
                duration_secs: 60,
                level: 3
            }
        );
    }

    #[test]
    fn try_load_from_reads_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let s = Settings {
            duration_secs: 30,
            level: 2,
        };
        s.save_to(&path).unwrap();
        assert_eq!(Settings::try_load_from(&path), Some(s));
    }

    #[test]
    fn try_load_from_missing_file_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");
        assert_eq!(Settings::try_load_from(&path), None);
    }

    #[test]
    fn try_load_from_malformed_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Settings::try_load_from(&path), None);
    }

    #[test]
    fn is_valid_enforces_ranges() {
        assert!(Settings {
            duration_secs: 60,
            level: 3
        }
        .is_valid());
        assert!(!Settings {
            duration_secs: 60,
            level: 0
        }
        .is_valid()); // level too low
        assert!(!Settings {
            duration_secs: 60,
            level: 6
        }
        .is_valid()); // level too high
        assert!(!Settings {
            duration_secs: 4,
            level: 3
        }
        .is_valid()); // duration too short
        assert!(!Settings {
            duration_secs: 601,
            level: 3
        }
        .is_valid()); // duration too long
    }

    #[test]
    fn out_of_range_settings_file_is_rejected() {
        // Mirrors load_existing's `try_load_from(..).filter(is_valid)` without the
        // home-directory dependency: a parseable but out-of-range file is ignored.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"duration_secs":60,"level":0}"#).unwrap();
        assert_eq!(
            Settings::try_load_from(&path).filter(Settings::is_valid),
            None
        );
    }
}
