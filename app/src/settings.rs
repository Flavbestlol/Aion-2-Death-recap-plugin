//! What the player chose, kept in Death Recap's own data folder
//! (`%APPDATA%\com.flavbest.death-recap\settings.json`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How long Death Recap remembers who killed you.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum History {
    /// Until Death Recap closes; nothing written to disk.
    #[default]
    Session,
    Day,
    ThreeDays,
}

impl History {
    /// How long a kill is kept, in ms (`None`: this session only).
    pub fn keep_ms(self) -> Option<i64> {
        match self {
            History::Session => None,
            History::Day => Some(24 * 3_600_000),
            History::ThreeDays => Some(3 * 24 * 3_600_000),
        }
    }
}

/// How Death Recap looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// The HUD look every Flav-Toolkit add-on shares: warm white and gold on
    /// dark glass, rounded, round portraits.
    #[default]
    Hud,
    /// The game's own look: charcoal, square corners, teal, the game's font.
    Game,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Look attackers up on the official AION 2 site (gear score, portrait,
    /// gear). Off unless the player turns it on.
    pub online_lookups: bool,
    pub history: History,
    /// Show survived fights too, not only deaths and lost duels.
    pub every_fight: bool,
    pub theme: Theme,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { online_lookups: false, history: History::Session, every_fight: false, theme: Theme::Hud }
    }
}

impl Settings {
    pub fn path(dir: &Path) -> PathBuf {
        dir.join("settings.json")
    }

    /// The saved settings; on the first start, what the player had chosen
    /// in Flav-Toolkit when Death Recap was built into it (read once, never
    /// written there); the defaults otherwise.
    pub fn load(dir: &Path, core_dir: Option<&Path>) -> Settings {
        if let Some(s) = read(&Self::path(dir)) {
            return s;
        }
        let from_core = core_dir.and_then(|d| read(&d.join("settings.json")));
        let s = from_core.unwrap_or_default();
        s.save(dir);
        s
    }

    pub fn save(&self, dir: &Path) {
        let _ = std::fs::create_dir_all(dir);
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let tmp = dir.join("settings.json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, Self::path(dir));
            }
        }
    }
}

/// Settings from a file; the fields Death Recap does not know (the core's
/// own, in its file) are ignored.
fn read(path: &Path) -> Option<Settings> {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("death-recap-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The first start takes the player's choices from the core's file (its
    /// own fields ignored), then keeps its own.
    #[test]
    fn the_first_start_takes_the_choices_made_in_the_core() {
        let (ours, core) = (dir("ours"), dir("core"));
        std::fs::write(
            core.join("settings.json"),
            r#"{"online_lookups":true,"history":"day","every_fight":false,"icon":true,"icon_at":[1500,12],"shop":true,"addon_roots":[]}"#,
        )
        .unwrap();
        let s = Settings::load(&ours, Some(&core));
        assert!(s.online_lookups);
        assert_eq!((s.history, s.theme), (History::Day, Theme::Hud), "the HUD look unless chosen");
        // Its own file now; a change in the core's no longer matters.
        std::fs::write(core.join("settings.json"), r#"{"online_lookups":false}"#).unwrap();
        assert!(Settings::load(&ours, Some(&core)).online_lookups);
        let _ = std::fs::remove_dir_all(&ours);
        let _ = std::fs::remove_dir_all(&core);
    }

    /// A file from 0.2.0 (with the skull icon's settings, gone since) keeps
    /// every choice it can.
    #[test]
    fn a_file_from_before_keeps_its_choices() {
        let ours = dir("before");
        std::fs::write(
            Settings::path(&ours),
            r#"{"online_lookups":true,"history":"three_days","every_fight":true,"icon":false,"icon_at":[10,10]}"#,
        )
        .unwrap();
        let s = Settings::load(&ours, None);
        assert_eq!(
            s,
            Settings { online_lookups: true, history: History::ThreeDays, every_fight: true, theme: Theme::Hud }
        );
        let _ = std::fs::remove_dir_all(&ours);
    }

    #[test]
    fn lookups_are_off_unless_chosen() {
        let ours = dir("fresh");
        assert!(!Settings::load(&ours, None).online_lookups);
        let _ = std::fs::remove_dir_all(&ours);
    }
}
