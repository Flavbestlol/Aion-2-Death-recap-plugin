//! What Death Recap's parts share: the settings, who killed you, the last
//! recap, and what the official site said about its attackers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use armory::{Armory, Item};
use recap::view::{Kill, KillLog, RecapView};
use serde::Serialize;

use crate::settings::Settings;

/// What the overlay shows for one fight.
#[derive(Debug, Clone, Serialize)]
pub struct Recap {
    /// The fight's end, Unix ms: names the fight in later messages.
    pub fight_id: i64,
    pub view: RecapView,
    /// Each attacker's kills on you in the history kept, newest first.
    pub history: HashMap<String, Vec<Kill>>,
    /// What the official site says about each attacker, as it arrives.
    pub extras: HashMap<i32, Extra>,
}

/// One attacker, from the official site.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Extra {
    pub gear_score: Option<i64>,
    pub combat_power: Option<i64>,
    pub portrait: Option<String>,
    pub items: Vec<Item>,
    /// There is an official character page to open.
    pub page: bool,
}

pub struct Shared {
    /// Death Recap's data folder.
    pub dir: PathBuf,
    pub settings: Mutex<Settings>,
    pub kills: Mutex<KillLog>,
    pub last: Mutex<Option<Recap>>,
    /// Official character pages, by fight and attacker.
    pub pages: Mutex<HashMap<(i64, i32), String>>,
    pub armory: Armory,
    /// Where the connection to Flav-Toolkit is, in words.
    pub status: Mutex<String>,
    /// Fights recorded since Death Recap started (for its line in the hub).
    pub fights: AtomicUsize,
}

impl Shared {
    /// `core_dir`: Flav-Toolkit's data folder, read once on the first start
    /// for the choices and the history kept when Death Recap was built in.
    pub fn new(dir: PathBuf, core_dir: Option<&Path>) -> Arc<Shared> {
        let first_start = !Settings::path(&dir).exists();
        let settings = Settings::load(&dir, core_dir);
        let mut kills = KillLog::default();
        if let Some(keep) = settings.history.keep_ms() {
            let saved = read_history(&dir.join("history.json")).or_else(|| {
                if first_start { core_dir.and_then(|d| read_history(&d.join("history.json"))) } else { None }
            });
            if let Some(saved) = saved {
                kills = saved;
                kills.prune(now_ms(), keep);
            }
        }
        let shared = Arc::new(Shared {
            dir,
            settings: Mutex::new(settings),
            kills: Mutex::new(kills),
            last: Mutex::new(None),
            pages: Mutex::new(HashMap::new()),
            armory: Armory::new(),
            status: Mutex::new("Starting".into()),
            fights: AtomicUsize::new(0),
        });
        shared.save_history();
        shared
    }

    /// Keep the kill history on disk when the player chose to (24 hours or 3
    /// days), and nothing on disk otherwise.
    pub fn save_history(&self) {
        let keep = lock(&self.settings).history.keep_ms();
        let path = self.dir.join("history.json");
        match keep {
            Some(keep) => {
                let mut kills = lock(&self.kills);
                kills.prune(now_ms(), keep);
                if let Ok(text) = serde_json::to_string(&*kills) {
                    let _ = std::fs::create_dir_all(&self.dir);
                    let _ = std::fs::write(path, text);
                }
            }
            None => {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    pub fn save_settings(&self) {
        lock(&self.settings).save(&self.dir);
    }
}

fn read_history(path: &Path) -> Option<KillLog> {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok())
}

pub fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
