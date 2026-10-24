//! Death Recap against a real feed, in this process: the core's feed serves
//! the live Abyss death (the fixture's events, as the core read them), and
//! the reader shows the recap, writes its line in the hub, and raises its note.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use death_recap::reader::{self, Screen};
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
                    && let Some(n) = c.notes.iter().find(|n| n.title == "You died")
                {
                    *found = Some((c.types.clone(), n.clone()));
                    feed.publish(&Event {
                        at_ms: last_ms + 20_000,
                        kind: EventKind::Hp { id: 12_445, hp: 5_000, source: HpSource::Combat },
                    });
                    let _ = feed.show("death-recap", Show::Settings);
                }
                false
            }
            // Then: the note taken back, and Settings reached it.
            (Some(_), Some(c)) => {
                (c.notes.iter().all(|n| n.title != "You died") && !seen.shown.lock().unwrap().is_empty())
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

    let (types, note) = in_hub.lock().unwrap().clone().expect("Death Recap's line and note in the hub");
    assert_eq!(types, reader::TYPES);
    assert_eq!(note.level, NoteLevel::Alert);
    // The fixture has ids only: the killer is "#12645", an Assassin, with Flash Slice.
    assert_eq!(note.text.as_deref(), Some("#12645 · Flash Slice"));
    assert_eq!(note.icon, Some(NoteIcon::Class("Assassin".into())));
    assert_eq!(*seen.shown.lock().unwrap(), vec![Show::Settings], "the hub's Settings reached Death Recap");
    assert!(
        feed.clients().iter().all(|c| c.notes.iter().all(|n| n.title != "You died")),
        "the note came down when the player was up again"
    );
    let _ = std::fs::remove_dir_all(&core_dir);
}

/// The manifest beside the program is one the hub lists and starts, and it
/// asks for exactly the types the reader reads.
#[test]
fn the_manifest_is_valid() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let found = flav_addons::read(&root);
    let m = found.manifest.clone().expect("addon.json");
    assert!(found.ok(), "{:?}", found.problems.iter().map(|p| p.to_string()).collect::<Vec<_>>());
    assert_eq!((m.id.as_str(), m.version.as_str()), (death_recap::ID, death_recap::VERSION));
    assert_eq!(m.events.map(|e| e.types).unwrap_or_default(), reader::TYPES);
}
