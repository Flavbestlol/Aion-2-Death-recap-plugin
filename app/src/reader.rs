//! The reading loop: Flav-Toolkit's feed in, each fight that ends in a death
//! or a lost duel out to the screen.
//!
//! It connects to the feed with `flav-sdk`, asks for the event types the
//! recap reads, hands every event to `recap::Recorder`, and builds the view
//! when a fight ends. While Flav-Toolkit is not running it waits and tries
//! again; when the core restarts it connects again (the core re-sends who the
//! player is at hello). It never talks to the game: the core reads it.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use flav_sdk::{Client, Config, Error, Event, EventKind, LogLevel, Message, NoteIcon, NoteLevel};
use recap::view::{Kill, Me, RecapView};
use recap::{Fight, Outcome, Recorder};

use crate::store::{Extra, Recap, Shared, lock};

/// The event types the recap reads: who is who, the player's HP and death,
/// hits, misses and buffs, and the player's own numbers to compare with.
pub const TYPES: &[&str] = &[
    "me",
    "name",
    "npc_identified",
    "player_profile",
    "player_server",
    "my_server",
    "my_combat_power",
    "my_gear_score",
    "max_hp",
    "hp",
    "died",
    "hit",
    "missed",
    "buff_added",
    "buff_changed",
    "buff_removed",
];

/// What the app shows. The Tauri windows in the app; a recorder in tests.
pub trait Screen: Send + Sync {
    /// A fight ended in a death or a lost duel (or any fight, if the player
    /// asked for every one): show it.
    fn recap(&self, recap: &Recap);
    /// The official site described an attacker of that fight.
    fn extra(&self, fight_id: i64, id: i32, extra: &Extra);
    /// The player is back on their feet: a death's recap can go.
    fn revived(&self);
    /// Where the connection is, in words.
    fn status(&self, _text: &str) {}
}

/// The game's time as the events tell it. A replay (`flav feed`) plays an
/// old session at its own pace, so "now" is the last event's time plus what
/// has passed since it came, never the wall clock.
#[derive(Default)]
struct Clock {
    last: Option<(i64, Instant)>,
}

impl Clock {
    fn saw(&mut self, at_ms: i64) {
        self.last = Some((at_ms, Instant::now()));
    }
    fn now(&self) -> Option<i64> {
        self.last.map(|(at, when)| at + when.elapsed().as_millis() as i64)
    }
}

/// What the loop knows beyond the recorder.
#[derive(Default)]
struct Reading {
    me: Me,
    my_server: Option<u16>,
    server_of: HashMap<String, u16>,
    /// The player is dead (the recap stays until they are back).
    dead: bool,
}

/// Runs until `stop` says so (never, in the app), connecting again whenever
/// Flav-Toolkit is not there.
pub fn run(shared: Arc<Shared>, screen: Arc<dyn Screen>, config: &Config, stop: &dyn Fn() -> bool) {
    let mut recorder = Recorder::new();
    let mut reading = Reading::default();
    let mut clock = Clock::default();
    loop {
        if stop() {
            return;
        }
        let mut client = match Client::connect(config) {
            Ok(c) => c,
            Err(Error::Refused(why)) => {
                set_status(&shared, &*screen, &format!("Flav-Toolkit refused: {why}"));
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
            Err(_) => {
                set_status(&shared, &*screen, "Waiting for Flav-Toolkit");
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
        };
        set_status(&shared, &*screen, &format!("Connected: {}", client.status.text));
        let _ = client.log(LogLevel::Info, &format!("Death Recap {} reading the feed", env!("CARGO_PKG_VERSION")));
        let mut last_note = Instant::now() - Duration::from_secs(10);
        loop {
            match client.next(Duration::from_millis(250)) {
                Ok(Some(Message::Event(e))) => {
                    clock.saw(e.at_ms);
                    for f in step(&mut recorder, &mut reading, &*screen, &e) {
                        ended(&shared, &screen, &mut recorder, &mut reading, &mut client, f);
                    }
                }
                Ok(Some(Message::Status(s))) => set_status(&shared, &*screen, &format!("Connected: {}", s.text)),
                // A type newer than this build: skipped.
                Ok(Some(Message::Unknown(_))) => {}
                Ok(None) => {
                    if let Some(now) = clock.now() {
                        for f in recorder.tick(now) {
                            ended(&shared, &screen, &mut recorder, &mut reading, &mut client, f);
                        }
                    }
                }
                Err(e) => {
                    set_status(&shared, &*screen, &format!("Flav-Toolkit went away ({e}); waiting for it"));
                    break;
                }
            }
            if last_note.elapsed() >= Duration::from_secs(2) {
                last_note = Instant::now();
                client.set_note(Some(&note(shared.fights.load(Ordering::Relaxed))));
            }
            if stop() {
                return;
            }
        }
    }
}

/// The words beside Death Recap in the hub.
pub fn note(fights: usize) -> String {
    match fights {
        1 => "1 fight recorded".into(),
        n => format!("{n} fights recorded"),
    }
}

fn set_status(shared: &Shared, screen: &dyn Screen, text: &str) {
    let mut status = lock(&shared.status);
    if *status != text {
        *status = text.to_string();
        screen.status(text);
    }
}

/// One event: the player's own numbers, back on their feet, then the recorder.
fn step(recorder: &mut Recorder, reading: &mut Reading, screen: &dyn Screen, e: &Event) -> Vec<Fight> {
    match &e.kind {
        EventKind::MyCombatPower { combat_power, .. } => reading.me.combat_power = Some(*combat_power),
        EventKind::MyGearScore { gear_score, .. } => reading.me.gear_score = Some(*gear_score),
        EventKind::MyServer { server } => reading.my_server = Some(*server),
        EventKind::PlayerServer { name, server } => {
            reading.server_of.insert(name.clone(), *server);
        }
        EventKind::Hp { id, hp, .. } if reading.dead && Some(*id) == recorder.me() && *hp > 0 => {
            reading.dead = false;
            screen.revived();
        }
        _ => {}
    }
    recorder.push(e)
}

/// A fight ended: who killed you into the history, the view to the screen,
/// and, when the player turned them on, the lookups on the official site.
fn ended(
    shared: &Arc<Shared>,
    screen: &Arc<dyn Screen>,
    recorder: &mut Recorder,
    reading: &mut Reading,
    client: &mut Client,
    mut fight: Fight,
) {
    recorder.fill_in(&mut fight);
    let every = lock(&shared.settings).every_fight;
    if fight.outcome == Outcome::Survived && !every {
        return;
    }
    shared.fights.fetch_add(1, Ordering::Relaxed);
    let book = recorder.book();
    let name_of = |id: i32| {
        fight
            .attackers
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.name.clone())
            .or_else(|| book.name(id).map(str::to_string))
    };
    // Who killed you, for the history.
    let blow = fight.final_blow().cloned();
    if let Some(blow) = &blow
        && fight.attackers.iter().any(|a| a.id == blow.actor && a.player)
        && let Some(killer) = name_of(blow.actor)
    {
        let server = fight.attackers.iter().find(|a| a.id == blow.actor).and_then(|a| a.server);
        lock(&shared.kills).add(Kill {
            at_ms: fight.end_ms,
            server: server.or_else(|| reading.server_of.get(&killer).copied()),
            killer,
            skill: book.skill(blow.skill).and_then(|s| s.name.clone()),
            duel: fight.outcome == Outcome::DuelLost,
        });
        shared.save_history();
    }
    let view = {
        let kills = lock(&shared.kills);
        RecapView::build(&fight, book, reading.me, &|name| kills.count(name))
    };
    let history = {
        let kills = lock(&shared.kills);
        view.attackers
            .iter()
            .filter(|a| a.player)
            .map(|a| (a.name.clone(), kills.by(&a.name).into_iter().take(10).cloned().collect()))
            .collect()
    };
    let recap = Recap { fight_id: fight.end_ms, view, history, extras: HashMap::new() };
    *lock(&shared.last) = Some(recap.clone());
    if fight.outcome == Outcome::Died {
        reading.dead = true;
    }
    screen.recap(&recap);

    // A note in the hub (and on the dock's icon): who it was.
    if fight.outcome != Outcome::Survived
        && let Some(k) = recap.view.attackers.iter().find(|a| a.killer).or(recap.view.attackers.first())
    {
        let how = recap.view.final_blow.as_ref().map(|b| format!(" · {}", b.skill.name)).unwrap_or_default();
        let icon = match &k.class {
            Some(class) if k.player => NoteIcon::Class(class.clone()),
            _ => NoteIcon::Addon,
        };
        let title = if fight.outcome == Outcome::DuelLost { "Duel lost" } else { "Last death" };
        let _ = client.notify(NoteLevel::News, title, Some(&format!("{}{how}", k.name)), Some(icon));
    }

    if lock(&shared.settings).online_lookups {
        let players: Vec<(i32, String, Option<u16>)> = recap
            .view
            .attackers
            .iter()
            .filter(|a| a.player && !a.name.starts_with('#'))
            .map(|a| (a.id, a.name.clone(), a.server.or_else(|| reading.server_of.get(&a.name).copied())))
            .collect();
        let region = reading.my_server.and_then(armory::server).map(|s| s.region);
        let (shared, screen, fight_id) = (shared.clone(), screen.clone(), recap.fight_id);
        std::thread::spawn(move || look_up(&shared, &*screen, fight_id, players, region));
    }
}

/// Gear score, portrait and gear of each attacker, from the official site,
/// one after the other (the armory paces its requests).
fn look_up(
    shared: &Shared,
    screen: &dyn Screen,
    fight_id: i64,
    players: Vec<(i32, String, Option<u16>)>,
    region: Option<String>,
) {
    for (id, name, server) in players {
        let profile = match (server, &region) {
            (Some(s), _) => shared.armory.lookup(&name, s),
            (None, Some(r)) => shared.armory.lookup_in_region(&name, r),
            (None, None) => continue,
        };
        let Ok(profile) = profile else { continue };
        let items = shared.armory.equipment(&profile).unwrap_or_default();
        let page = armory::page_url(&profile);
        if let Some(page) = &page {
            lock(&shared.pages).insert((fight_id, id), page.clone());
        }
        let extra = Extra {
            gear_score: profile.gear_score,
            combat_power: Some(profile.combat_power),
            portrait: profile.portrait.clone(),
            items,
            page: page.is_some(),
        };
        if let Some(last) = lock(&shared.last).as_mut()
            && last.fight_id == fight_id
        {
            last.extras.insert(id, extra.clone());
        }
        screen.extra(fight_id, id, &extra);
    }
}
