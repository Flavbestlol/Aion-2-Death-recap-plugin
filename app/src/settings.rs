//! What the player chose, kept in Death Recap's own data folder
//! (`%APPDATA%\com.flavbest.death-recap\settings.json`).

use std::path::{Path, PathBuf};

use flav_sdk::{Control, SettingItem, SettingsGroup, SharedLink};
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
        let path = Self::path(dir);
        if path.exists() {
            if let Some(s) = read(&path) {
                return s;
            }
            // Damaged: the defaults (lookups off), and the file kept aside
            // for a look. The core's old choices are for the first start only.
            let _ = std::fs::rename(&path, dir.join("settings.json.damaged"));
            let s = Settings::default();
            s.save(dir);
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

impl History {
    /// Its name in `settings.json` and in the hub.
    fn key(self) -> &'static str {
        match self {
            History::Session => "session",
            History::Day => "day",
            History::ThreeDays => "three_days",
        }
    }
}

impl Theme {
    /// Its name in `settings.json`, and the shared `look`'s value for it.
    fn key(self) -> &'static str {
        match self {
            Theme::Hud => "hud",
            Theme::Game => "game",
        }
    }
}

fn choices(list: &[(&str, &str)]) -> Vec<[String; 2]> {
    list.iter().map(|(v, name)| [v.to_string(), name.to_string()]).collect()
}

fn item(key: &str, name: &str, help: Option<&str>, control: Control) -> SettingItem {
    SettingItem {
        key: key.into(),
        name: name.into(),
        help: help.map(str::to_string),
        control,
        shared: None,
        disabled: None,
    }
}

impl Settings {
    /// The settings as Flav-Toolkit's hub shows them (the feed's `settings`,
    /// core 0.18.0): the same four the panel has, with the same words. The
    /// look is the shared `look`, so the player can set it for every add-on
    /// at once; its values are already the shared ones (`hud`, `game`).
    pub fn describe(&self) -> Vec<SettingsGroup> {
        let lookups = SettingsGroup {
            id: "lookups".into(),
            name: "Lookups".into(),
            items: vec![
                item(
                    "online_lookups",
                    "Look up attackers online",
                    Some(
                        "Gear score, portrait and gear, from the official AION 2 site. Sends their name and server, nothing about you",
                    ),
                    Control::Switch { value: self.online_lookups },
                ),
                item(
                    "every_fight",
                    "Show after every fight",
                    Some("Not only after a death or a lost duel"),
                    Control::Switch { value: self.every_fight },
                ),
                item(
                    "history",
                    "Remember who killed you",
                    None,
                    Control::Choice {
                        choices: choices(&[("session", "This session"), ("day", "A day"), ("three_days", "3 days")]),
                        value: self.history.key().into(),
                    },
                ),
            ],
        };
        let mut theme = item(
            "theme",
            "Look",
            Some("The HUD every add-on shares, or the game's own"),
            Control::Choice { choices: choices(&[("hud", "HUD"), ("game", "Game")]), value: self.theme.key().into() },
        );
        theme.shared = Some(SharedLink { id: "look".into(), map: Default::default(), scale: None });
        let look = SettingsGroup { id: "look".into(), name: "Look".into(), items: vec![theme] };
        vec![lookups, look]
    }

    /// One change by key, from the hub (the feed's `setting`) or the panel:
    /// `true` when it was one of ours with a value it takes. Anything else
    /// (an unknown key, a value of the wrong kind, a history or look that
    /// isn't offered) changes nothing.
    pub fn apply(&mut self, key: &str, value: &serde_json::Value) -> bool {
        match key {
            "online_lookups" => value.as_bool().map(|on| self.online_lookups = on).is_some(),
            "every_fight" => value.as_bool().map(|on| self.every_fight = on).is_some(),
            "history" => match value.as_str() {
                Some("session") => set(&mut self.history, History::Session),
                Some("day") => set(&mut self.history, History::Day),
                Some("three_days") => set(&mut self.history, History::ThreeDays),
                _ => false,
            },
            "theme" => match value.as_str() {
                Some("hud") => set(&mut self.theme, Theme::Hud),
                Some("game") => set(&mut self.theme, Theme::Game),
                _ => false,
            },
            _ => false,
        }
    }
}

fn set<T>(slot: &mut T, value: T) -> bool {
    *slot = value;
    true
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

    /// A damaged file is not a first start: the defaults, never the core's
    /// old choices.
    #[test]
    fn a_damaged_file_gives_the_defaults() {
        let (ours, core) = (dir("damaged"), dir("damaged-core"));
        std::fs::write(Settings::path(&ours), r#"{"history":"week"}"#).unwrap();
        std::fs::write(core.join("settings.json"), r#"{"online_lookups":true}"#).unwrap();
        let s = Settings::load(&ours, Some(&core));
        assert!(!s.online_lookups);
        assert!(ours.join("settings.json.damaged").exists());
        let _ = std::fs::remove_dir_all(&ours);
        let _ = std::fs::remove_dir_all(&core);
    }

    /// What the hub gets is what the core keeps: the core's own check
    /// (`clean`) drops nothing, the look is linked to the shared `look` and
    /// can reach both its values, and every value is the setting's own.
    #[test]
    fn the_hub_gets_all_four_settings() {
        use flav_events::settings::{clean, link_fits};
        let s = Settings { online_lookups: true, history: History::ThreeDays, every_fight: false, theme: Theme::Game };
        let groups = s.describe();
        assert_eq!(clean(groups.clone()), groups, "the core keeps the description as it is");
        let items: Vec<_> = groups.iter().flat_map(|g| &g.items).collect();
        let keys: Vec<_> = items.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, ["online_lookups", "every_fight", "history", "theme"]);
        assert_eq!(items[0].control, Control::Switch { value: true });
        assert_eq!(items[1].control, Control::Switch { value: false });
        assert!(matches!(&items[2].control, Control::Choice { value, .. } if value == "three_days"));
        let theme = items[3];
        assert!(matches!(&theme.control, Control::Choice { value, .. } if value == "game"));
        let link = theme.shared.as_ref().expect("the look is shared");
        assert_eq!(link.id, "look");
        assert!(link_fits(link, &theme.control), "every shared look has a value here");
        assert!(items[..3].iter().all(|i| i.shared.is_none()), "only the look is shared");
    }

    /// A change from the hub sets exactly that setting, with the same names
    /// the file and the panel use; anything else changes nothing.
    #[test]
    fn a_change_from_the_hub_sets_only_what_it_names() {
        use serde_json::json;
        let mut s = Settings::default();
        assert!(s.apply("online_lookups", &json!(true)));
        assert!(s.apply("every_fight", &json!(true)));
        assert!(s.apply("history", &json!("day")));
        assert!(s.apply("theme", &json!("game")));
        assert_eq!(s, Settings { online_lookups: true, history: History::Day, every_fight: true, theme: Theme::Game });
        // The names are the file's: what the hub sends, `settings.json` reads.
        let text = serde_json::to_value(&s).unwrap();
        assert_eq!((text["history"].as_str(), text["theme"].as_str()), (Some("day"), Some("game")));

        let before = s.clone();
        for (key, value) in [
            ("online_lookups", json!("yes")),
            ("history", json!("week")),
            ("theme", json!("apple")),
            ("theme", json!(1)),
            ("volume", json!(1)),
        ] {
            assert!(!s.apply(key, &value), "{key} = {value}");
        }
        assert_eq!(s, before, "nothing changed");
        // Every value the description offers is one `apply` takes.
        for group in Settings::default().describe() {
            for i in group.items {
                if let Control::Choice { choices, .. } = &i.control {
                    for [v, _] in choices {
                        assert!(Settings::default().apply(&i.key, &json!(v)), "{} = {v}", i.key);
                    }
                }
            }
        }
    }

    #[test]
    fn lookups_are_off_unless_chosen() {
        let ours = dir("fresh");
        assert!(!Settings::load(&ours, None).online_lookups);
        let _ = std::fs::remove_dir_all(&ours);
    }
}
