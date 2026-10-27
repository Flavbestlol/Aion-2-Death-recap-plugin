//! Death Recap against a real feed, in this process: the core's feed serves
//! the live Abyss death (the fixture's events, as the core read them), and
//! the reader shows the recap, writes its line in the hub, and raises its note.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use death_recap::reader::{self, Screen};
use death_recap::settings::{History, Theme};
use death_recap::store::{Extra, Recap, Shared};
use flav_events::event::HpSource;
use flav_events::feed::{CoreStatus, NoteIcon, NoteLevel, Show};
use flav_events::{Event, EventKind};
use flav_feed::{Feed, Options};

const ABYSS_LIVE: &str = include_str!("../../crates/recap/tests/fixtures/abyss-death-live-2026-10-21.events.jsonl");

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("death-recap-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[derive(Default)]
struct Seen {
    recaps: Mutex<Vec<Recap>>,
    shown: Mutex<Vec<Show>>,
}

impl Screen for Seen {
    fn recap(&self, recap: &Recap) {
        self.recaps.lock().unwrap().push(recap.clone());
    }
    fn extra(&self, _: i64, _: i32, _: &Extra) {}
    fn revived(&self) {}
    fn show(&self, what: Show) {
        self.shown.lock().unwrap().push(what);
    }
}

/// The live Abyss death through a real feed: the recap; Death Recap's line
/// in the hub; its alert note with the killer's class emblem, taken back
/// when the player is up again; and the hub's Settings button reaching it.
#[test]
fn a_death_on_the_feed_becomes_a_recap_and_a_note() {
    let core_dir = dir("core");
    let feed = Arc::new(Feed::start(Options { dir: core_dir.clone(), port: 0, core: "test".into() }).unwrap());
    feed.set_status(CoreStatus { capturing: true, game: true, text: "Reading the game".into(), ..Default::default() });
    let events: Vec<Event> =
        ABYSS_LIVE.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).unwrap()).collect();
    let last_ms = events.last().map(|e| e.at_ms).unwrap_or(0);

    // The feed publishes the session from another thread once Death Recap is in.
    let publisher = {
        let feed = Arc::clone(&feed);
        std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            while feed.clients().is_empty() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(20));
            }
            for e in &events {
                feed.publish(e);
            }
        })
    };

    let shared = Shared::new(dir("data"), None);
    let seen = Arc::new(Seen::default());
    let screen: Arc<dyn Screen> = seen.clone();
    let mut config = death_recap::config();
    config.dir = Some(core_dir.clone());
    let deadline = Instant::now() + Duration::from_secs(40);
    let in_hub = Mutex::new(None);
    let stop = || {
        let me = feed.clients().into_iter().find(|c| c.addon == "death-recap");
        let mut found = in_hub.lock().unwrap();
        match (&*found, me) {
            // First: the fight and the note, as the hub sees them. Then the
            // player stands up again, and the player presses Settings.
            (None, Some(c)) => {
                if c.note.as_deref() == Some("1 fight recorded")
                    && let Some(n) = c.notes.iter().find(|n| n.title.starts_with("Killed by "))
                {
                    *found = Some((c.types.clone(), n.clone()));
                    feed.publish(&Event {
                        at_ms: last_ms + 20_000,
                        kind: EventKind::Hp { id: 12_445, hp: 5_000, source: HpSource::Combat },
                    });
                    let _ = feed.show("death-recap", Show::Settings);
                    // The dock's icon: Death Recap asked for `toggle`, so it
                    // gets `Toggle` (an add-on that did not would get `Open`).
                    let _ = feed.show("death-recap", Show::Toggle);
                }
                false
            }
            // Then: the note taken back, and Settings and the dock's click reached it.
            (Some(_), Some(c)) => {
                (c.notes.iter().all(|n| !n.title.starts_with("Killed by ")) && seen.shown.lock().unwrap().len() >= 2)
                    || Instant::now() > deadline
            }
            _ => Instant::now() > deadline,
        }
    };
    reader::run(shared.clone(), screen, &config, &stop);
    publisher.join().unwrap();

    let recaps = seen.recaps.lock().unwrap();
    assert_eq!(recaps.len(), 1, "one death");
    let v = &recaps[0].view;
    assert_eq!(v.attackers.iter().map(|a| a.id).collect::<Vec<_>>(), vec![10_159, 12_645]);
    assert_eq!(v.final_blow.as_ref().map(|b| (b.skill.name.as_str(), b.damage)), Some(("Flash Slice", 647)));
    assert_eq!(v.damage, 22_992);

    // With the killer's combat power known (it comes with them into view),
    // the note says it, against the player's.
    let mut known = v.clone();
    let k = known.attackers.iter_mut().find(|a| a.killer).unwrap();
    (k.name, k.combat_power, k.combat_power_vs_me) = ("Sampleton".into(), Some(140_596), Some(20));
    let n = reader::death_note(&known).unwrap();
    assert_eq!(n.title, "Killed by Sampleton · 140.6K CP");
    assert_eq!(n.text, "Assassin · 140,596 combat power, 20% above yours · Flash Slice 647");
    known.outcome = recap::Outcome::DuelLost;
    assert_eq!(reader::death_note(&known).unwrap().title, "Duel lost to Sampleton · 140.6K CP");
    known.outcome = recap::Outcome::Survived;
    assert!(reader::death_note(&known).is_none(), "no note for a fight survived");

    let (types, note) = in_hub.lock().unwrap().clone().expect("Death Recap's line and note in the hub");
    assert_eq!(types, reader::TYPES);
    assert_eq!(note.level, NoteLevel::Alert);
    // The fixture has ids only: the killer is "#12645", an Assassin, with Flash Slice.
    assert_eq!(note.title, "Killed by #12645");
    assert_eq!(note.text.as_deref(), Some("Assassin · Flash Slice 647"));
    assert_eq!(note.icon, Some(NoteIcon::Class("Assassin".into())));
    assert_eq!(
        *seen.shown.lock().unwrap(),
        vec![Show::Settings, Show::Toggle],
        "the hub's Settings reached Death Recap, and the dock's click as a toggle"
    );
    assert!(
        feed.clients().iter().all(|c| c.notes.iter().all(|n| !n.title.starts_with("Killed by "))),
        "the note came down when the player was up again"
    );
    let _ = std::fs::remove_dir_all(&core_dir);
}

/// Its settings in the hub, through a real 0.18.0 feed: described on
/// connecting (the look linked to the shared `look`); a change from the hub
/// applied, saved and described again; a change from its own panel described
/// to the hub too. Both places always show the same values.
#[test]
fn the_hub_shows_and_changes_its_settings() {
    #[derive(Default)]
    struct Changes(Mutex<usize>);
    impl Screen for Changes {
        fn recap(&self, _: &Recap) {}
        fn extra(&self, _: i64, _: i32, _: &Extra) {}
        fn revived(&self) {}
        fn settings_changed(&self) {
            *self.0.lock().unwrap() += 1;
        }
    }
    let core_dir = dir("settings-core");
    let feed = Feed::start(Options { dir: core_dir.clone(), port: 0, core: "test".into() }).unwrap();
    let data = dir("settings-data");
    let shared = Shared::new(data.clone(), None);
    let changes = Arc::new(Changes::default());
    let screen: Arc<dyn Screen> = changes.clone();
    let mut config = death_recap::config();
    config.dir = Some(core_dir.clone());

    // What the hub holds for Death Recap: key → value, and the look's link.
    let in_hub = || {
        feed.addon_settings().get("death-recap").map(|d| {
            let items: Vec<_> = d.groups.iter().flat_map(|g| g.items.clone()).collect();
            let values: HashMap<String, serde_json::Value> = items
                .iter()
                .map(|i| (i.key.clone(), serde_json::to_value(&i.control).unwrap()["value"].clone()))
                .collect();
            let look = items.iter().find(|i| i.key == "theme").and_then(|i| i.shared.clone()).map(|l| l.id);
            (values, look)
        })
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    let step = Mutex::new(0);
    let first = Mutex::new(None);
    let stop = || {
        let mut step = step.lock().unwrap();
        match (*step, in_hub()) {
            // Described on connecting, as the settings are: the defaults.
            (0, Some(seen)) => {
                *first.lock().unwrap() = Some(seen);
                feed.send_setting("death-recap", "theme", "game".into()).unwrap();
                feed.send_setting("death-recap", "history", "day".into()).unwrap();
                *step = 1;
            }
            // The hub's changes applied and described again; then one from
            // the panel.
            (1, Some((v, _))) if v["theme"] == "game" && v["history"] == "day" => {
                assert!(shared.change("every_fight", &serde_json::Value::Bool(true)));
                *step = 2;
            }
            (2, Some((v, _))) if v["every_fight"] == true => return true,
            _ => {}
        }
        Instant::now() > deadline
    };
    reader::run(shared.clone(), screen, &config, &stop);

    let (values, look) = first.lock().unwrap().clone().expect("Death Recap described its settings");
    assert_eq!(values["online_lookups"], false, "lookups off unless chosen");
    assert_eq!((values["theme"].as_str(), values["history"].as_str()), (Some("hud"), Some("session")));
    assert_eq!(values.len(), 4);
    assert_eq!(look.as_deref(), Some("look"), "the look is the shared one");

    let (now, _) = in_hub().unwrap();
    assert_eq!(
        (now["theme"].as_str(), now["history"].as_str(), now["every_fight"].as_bool()),
        (Some("game"), Some("day"), Some(true)),
        "the hub shows what Death Recap keeps"
    );
    let saved = death_recap::settings::Settings::load(&data, None);
    assert_eq!((saved.theme, saved.history, saved.every_fight), (Theme::Game, History::Day, true), "saved to its file");
    assert!(data.join("history.json").exists(), "a day of history is kept on disk now");
    assert!(*changes.0.lock().unwrap() >= 2, "the windows followed both changes from the hub");
    let _ = std::fs::remove_dir_all(&core_dir);
    let _ = std::fs::remove_dir_all(&data);
}

/// The manifest beside the program is one the hub lists and starts, it asks
/// for exactly the types the reader reads, and the hub can show its emblem.
#[test]
fn the_manifest_is_valid() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let found = flav_addons::read(&root);
    let m = found.manifest.clone().expect("addon.json");
    assert!(found.ok(), "{:?}", found.problems.iter().map(|p| p.to_string()).collect::<Vec<_>>());
    assert_eq!((m.id.as_str(), m.version.as_str()), (death_recap::ID, death_recap::VERSION));
    assert!(flav_addons::emblem(&m, &root).is_some(), "emblem.png beside addon.json, a PNG of at most 64 KB");
    assert_eq!(m.events.map(|e| e.types).unwrap_or_default(), reader::TYPES);
}

/// A portrait's address on the official site (`profileimg.plaync.com`)
/// answers with a redirect to the picture itself on `fizz-download.playnccdn.com`
/// (checked 2026-10-27). The window follows it only when both hosts may load
/// pictures; without the second every portrait fell back to the class emblem
/// (0.3.0 to 0.3.3). Every host the window loads from is also one `addon.json`
/// declares, so the hub shows it.
#[test]
fn portraits_can_load_and_every_host_is_declared() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let conf: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("tauri.conf.json")).unwrap()).unwrap();
    let csp = &conf["app"]["security"]["csp"];
    let img = csp["img-src"].as_str().unwrap();
    for host in ["https://profileimg.plaync.com", "https://fizz-download.playnccdn.com"] {
        assert!(img.split_whitespace().any(|s| s == host), "img-src lacks {host}");
    }
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("../addon.json")).unwrap()).unwrap();
    let declared: Vec<&str> =
        manifest["permissions"]["network"].as_array().unwrap().iter().filter_map(|h| h.as_str()).collect();
    for directive in ["img-src", "font-src", "connect-src"] {
        for source in csp[directive].as_str().unwrap_or("").split_whitespace() {
            if let Some(host) = source.strip_prefix("https://") {
                assert!(declared.contains(&host), "{directive} loads from {host}, which addon.json does not declare");
            }
        }
    }
}
