use clap::Parser;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}
