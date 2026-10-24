//! The online lookups against a made-up official site, with no network: the
//! order they go in, asking again when the site refuses or fails, and a new
//! fight taking over.

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use armory::{Armory, Pace, Transport};
use death_recap::lookups::{self, Found, Patience, Player, Request};

const EQUIPMENT: &str = include_str!("../../crates/armory/tests/fixtures/equipment-sample.json");

/// A site that knows every name but "Nobody", on Fregion. Each name can be
/// given statuses to answer first (a refusal, a failure), one per request.
#[derive(Clone, Default)]
struct Site {
    asked: Arc<Mutex<Vec<String>>>,
    first: Arc<Mutex<HashMap<String, VecDeque<u16>>>>,
}

impl Site {
    fn failing(self, name: &str, statuses: &[u16]) -> Self {
        self.first.lock().unwrap().insert(name.into(), statuses.iter().copied().collect());
        self
    }
    /// What was asked, in order: `search Name`, `info Name`, `gear Name`.
    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

fn param<'a>(url: &'a str, key: &str) -> &'a str {
    url.split(['?', '&']).find_map(|p| p.strip_prefix(&format!("{key}="))).unwrap_or_default()
}

impl Transport for Site {
    fn get(&self, url: &str) -> Result<(u16, String), String> {
        let (what, name) = if url.contains("/search/") {
            ("search", param(url, "keyword").to_string())
        } else if url.contains("/character/info") {
            ("info", param(url, "characterId").trim_start_matches("id-").to_string())
        } else {
            ("gear", param(url, "characterId").trim_start_matches("id-").to_string())
        };
        self.asked.lock().unwrap().push(format!("{what} {name}"));
        if let Some(status) = self.first.lock().unwrap().get_mut(&name).and_then(|q| q.pop_front()) {
            return Ok((status, String::new()));
        }
        let body = match what {
            "search" if name == "Nobody" => r#"{"list":[]}"#.to_string(),
            "search" => format!(
                r#"{{"list":[{{"characterId":"id-{name}","name":"<strong>{name}</strong>","serverId":1307,"serverName":"Fregion","level":40}}]}}"#
            ),
            "info" => format!(
                r#"{{"profile":{{"characterId":"id-{name}","characterName":"{name}","characterLevel":40,"className":"Ranger","combatPower":100000,"raceName":"Elyos","serverId":1307,"serverName":"Fregion","profileImage":"https://profileimg.plaync.com/{name}.png"}},"stat":{{"statList":[{{"type":"ItemLevel","value":2700}}]}}}}"#
            ),
            _ => EQUIPMENT.to_string(),
        };
        Ok((200, body))
    }
}

fn armory(site: &Site, hold: Duration) -> Armory<Site> {
    Armory::with(site.clone(), Pace { gap: Duration::ZERO, keep: Duration::from_secs(600), hold, max_hold: hold })
}

fn quick() -> Patience {
    Patience {
        first_wait: Duration::from_millis(5),
        max_wait: Duration::from_millis(20),
        tries: 4,
        give_up_after: Duration::from_secs(30),
    }
}

fn players(names: &[&str]) -> Vec<Player> {
    names
        .iter()
        .enumerate()
        .map(|(i, n)| Player { id: i as i32 + 1, name: n.to_string(), server: Some(1307) })
        .collect()
}

/// Runs the worker over these requests until it is done; what it found.
fn run(armory: &Armory<Site>, requests: Vec<Request>) -> Vec<Found> {
    let (tx, rx) = channel();
    for r in requests {
        tx.send(r).unwrap();
    }
    drop(tx);
    let found = Mutex::new(Vec::new());
    lookups::run(armory, rx, quick(), &|| true, &|f| found.lock().unwrap().push(f));
    found.into_inner().unwrap()
}

/// In the order given (the killer, the opener, the rest): everyone's
/// profile first, so every portrait is up before anyone's gear is asked.
#[test]
fn profiles_first_in_order_then_gear() {
    let site = Site::default();
    let found = run(
        &armory(&site, Duration::ZERO),
        vec![Request { fight_id: 1, players: players(&["Killer", "Opener", "Third"]), region: None }],
    );
    let order: Vec<(i32, bool)> = found.iter().map(|f| (f.id, f.extra.gear_ready)).collect();
    assert_eq!(order, vec![(1, false), (2, false), (3, false), (1, true), (2, true), (3, true)]);
    assert_eq!(
        site.asked().iter().filter(|a| a.starts_with("search")).cloned().collect::<Vec<_>>(),
        vec!["search Killer", "search Opener", "search Third"]
    );
    let killer = &found[0].extra;
    assert_eq!(killer.portrait.as_deref(), Some("https://profileimg.plaync.com/Killer.png"));
    assert_eq!((killer.combat_power, killer.gear_score, killer.page), (Some(100_000), Some(2_700), true));
    let gear = &found[3].extra;
    assert_eq!(gear.items.len(), 3);
    assert_eq!((gear.items[0].enchant, gear.items[0].exceed), (15, 5), "+15 and the diamond level");
    assert!(found.iter().all(|f| f.fight_id == 1));
}

/// "Too many" and a failing site are asked again until they answer; the
/// killer keeps their place at the front.
#[test]
fn a_refusal_or_a_failure_is_asked_again() {
    let site = Site::default().failing("Killer", &[503, 502]).failing("Opener", &[429]);
    let found = run(
        &armory(&site, Duration::ZERO),
        vec![Request { fight_id: 1, players: players(&["Killer", "Opener"]), region: None }],
    );
    let profiles: Vec<i32> = found.iter().filter(|f| !f.extra.gear_ready).map(|f| f.id).collect();
    assert_eq!(profiles, vec![1, 2], "both came, the killer first");
    assert_eq!(found.iter().filter(|f| f.extra.gear_ready && !f.extra.items.is_empty()).count(), 2);
    assert_eq!(site.asked().iter().filter(|a| *a == "search Killer").count(), 3);
}

/// A character the site does not have is not asked again, and the others
/// still come.
#[test]
fn no_such_character_is_not_asked_again() {
    let site = Site::default();
    let found = run(
        &armory(&site, Duration::ZERO),
        vec![Request { fight_id: 1, players: players(&["Nobody", "Second"]), region: None }],
    );
    assert!(found.iter().all(|f| f.id == 2));
    assert_eq!(found.len(), 2);
    assert_eq!(site.asked().iter().filter(|a| *a == "search Nobody").count(), 1);
}

/// A site that keeps failing is given up on after a few tries; its card
/// stops waiting for the gear.
#[test]
fn a_site_that_keeps_failing_is_given_up_on() {
    let site = Site::default().failing("Killer", &[500; 10]);
    let found =
        run(&armory(&site, Duration::ZERO), vec![Request { fight_id: 1, players: players(&["Killer"]), region: None }]);
    assert!(found.is_empty());
    assert_eq!(site.asked().len(), quick().tries as usize);
}

/// A new fight while the site makes the worker wait: the new fight's
/// attackers are looked up, what was left of the old one is dropped.
#[test]
fn a_new_fight_takes_over() {
    let site = Site::default().failing("Old", &[429]);
    let armory = armory(&site, Duration::from_millis(300));
    let (tx, rx) = channel();
    tx.send(Request { fight_id: 1, players: players(&["Old", "Older"]), region: None }).unwrap();
    let sender = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(60));
        tx.send(Request { fight_id: 2, players: players(&["New"]), region: None }).unwrap();
    });
    let found = Mutex::new(Vec::new());
    lookups::run(&armory, rx, quick(), &|| true, &|f| found.lock().unwrap().push(f));
    sender.join().unwrap();
    let found = found.into_inner().unwrap();
    assert_eq!(
        found.iter().map(|f| (f.fight_id, f.id, f.extra.gear_ready)).collect::<Vec<_>>(),
        vec![(2, 1, false), (2, 1, true)]
    );
    assert!(!site.asked().contains(&"search Older".to_string()));
}

/// Turned off by the player: what is left is dropped, nothing more is sent.
#[test]
fn turning_the_lookups_off_stops_them() {
    let site = Site::default();
    let armory = armory(&site, Duration::ZERO);
    let (tx, rx) = channel();
    tx.send(Request { fight_id: 1, players: players(&["First", "Second", "Third"]), region: None }).unwrap();
    drop(tx);
    let on = std::sync::atomic::AtomicBool::new(true);
    let found = Mutex::new(Vec::new());
    lookups::run(&armory, rx, quick(), &|| on.load(std::sync::atomic::Ordering::Relaxed), &|f| {
        // Off as soon as the first answer is in.
        on.store(false, std::sync::atomic::Ordering::Relaxed);
        found.lock().unwrap().push(f);
    });
    assert_eq!(found.into_inner().unwrap().len(), 1);
    assert_eq!(site.asked(), vec!["search First", "info First"], "nothing after the switch");
}

/// "Too many" is waited out without using up the tries a failing site gets.
#[test]
fn too_many_does_not_use_up_the_tries() {
    let site = Site::default().failing("Killer", &[429, 429, 429, 429, 429, 503, 503]);
    let found =
        run(&armory(&site, Duration::ZERO), vec![Request { fight_id: 1, players: players(&["Killer"]), region: None }]);
    assert!(found.iter().any(|f| f.id == 1), "found after 5 refusals and 2 failures (4 tries)");
}

/// A fight's lookups stop at its time limit, even in the middle of a wait.
#[test]
fn the_time_limit_holds_during_a_wait() {
    let site = Site::default().failing("Killer", &[429; 50]);
    let armory = armory(&site, Duration::from_secs(60));
    let (tx, rx) = channel();
    tx.send(Request { fight_id: 1, players: players(&["Killer"]), region: None }).unwrap();
    drop(tx);
    let patience = Patience { give_up_after: Duration::from_millis(200), ..quick() };
    let started = std::time::Instant::now();
    lookups::run(&armory, rx, patience, &|| true, &|_| {});
    assert!(started.elapsed() < Duration::from_secs(5), "a 60 s hold is cut at the 200 ms limit");
}
