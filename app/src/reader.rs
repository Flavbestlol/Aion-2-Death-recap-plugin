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
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};

use flav_sdk::{Client, Config, Error, Event, EventKind, LogLevel, Message, NoteIcon, NoteLevel, Show};
use recap::view::{Kill, Me, RecapView};
use recap::{Fight, Outcome, Recorder};

use crate::lookups::{self, Found, Patience, Player, Request};
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
    /// The player pressed the hub's Open (the last recap) or Settings.
    fn show(&self, _what: Show) {}
    /// A setting changed in the hub: the overlay takes the look, an open
    /// settings panel shows the new values.
    fn settings_changed(&self) {}
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
    /// The title of Death Recap's note in the hub and on the dock, while one
    /// is up (one per fight: the last death or lost duel).
    note: Option<String>,
    /// The player is back on their feet: the note comes down.
    back_up: bool,
}

/// Runs until `stop` says so (never, in the app), connecting again whenever
/// Flav-Toolkit is not there.
pub fn run(shared: Arc<Shared>, screen: Arc<dyn Screen>, config: &Config, stop: &dyn Fn() -> bool) {
    let lookups = start_lookups(&shared, &screen);
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
        // Its settings in the hub (core 0.18.0; an older core never granted
        // `settings`, and then nothing is sent).
        shared.settings_changed.store(false, Ordering::Relaxed);
        describe(&shared, &mut client);
        let mut last_note = Instant::now() - Duration::from_secs(10);
        loop {
            match client.next(Duration::from_millis(250)) {
                Ok(Some(Message::Event(e))) => {
                    clock.saw(e.at_ms);
                    for f in step(&mut recorder, &mut reading, &*screen, &e) {
                        ended(&shared, &*screen, &lookups, &mut recorder, &mut reading, &mut client, f);
                    }
                }
                Ok(Some(Message::Status(s))) => set_status(&shared, &*screen, &format!("Connected: {}", s.text)),
                Ok(Some(Message::Show(what))) => screen.show(what),
                // The player changed one in the hub: applied as the panel
                // applies it, then described again (the value Death Recap
                // kept, also when it took nothing from it).
                Ok(Some(Message::Setting { key, value })) => {
                    if shared.change(&key, &value) {
                        screen.settings_changed();
                    } else {
                        shared.settings_changed.store(true, Ordering::Relaxed);
                    }
                }
                // A type newer than this build: skipped.
                Ok(Some(Message::Unknown(_))) => {}
                // What only comes with a feature Death Recap does not ask for
                // (the shared lock, hotkeys, layout mode): never sent to it.
                Ok(Some(_)) => {}
                Ok(None) => {
                    if let Some(now) = clock.now() {
                        for f in recorder.tick(now) {
                            ended(&shared, &*screen, &lookups, &mut recorder, &mut reading, &mut client, f);
                        }
                    }
                }
                Err(e) => {
                    set_status(&shared, &*screen, &format!("Flav-Toolkit went away ({e}); waiting for it"));
                    break;
                }
            }
            // Back on their feet, or the recap closed: the death's note comes down.
            let closed = shared.closed.swap(false, Ordering::Relaxed);
            if (std::mem::take(&mut reading.back_up) || closed)
                && let Some(title) = reading.note.take()
            {
                let _ = client.unnotify(Some(&title));
            }
            // Changed in the hub or in the panel: the hub shows the values
            // Death Recap keeps.
            if shared.settings_changed.swap(false, Ordering::Relaxed) {
                describe(&shared, &mut client);
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

/// Death Recap's note on a death or a lost duel.
#[derive(Debug, Clone, PartialEq)]
pub struct DeathNote {
    /// What the dock's tip and the hub show first: who, and how strong.
    pub title: String,
    /// The class, the power against yours, the final blow.
    pub text: String,
    pub icon: NoteIcon,
}

/// The note for a fight that ended in a death or a lost duel:
/// *Killed by Sampleton · 140.6K CP*, then *Assassin · 140,596 combat power,
/// 20% above yours · Flash Slice 5,677 crit*.
pub fn death_note(view: &RecapView) -> Option<DeathNote> {
    let duel = match view.outcome {
        Outcome::Died => false,
        Outcome::DuelLost => true,
        Outcome::Survived => return None,
    };
    let k = view.attackers.iter().find(|a| a.killer).or(view.attackers.first())?;
    let cp = k.combat_power.filter(|_| k.monster.is_none());
    let power = cp.map(|cp| format!(" · {} CP", short(cp))).unwrap_or_default();
    let title = if duel { format!("Duel lost to {}{power}", k.name) } else { format!("Killed by {}{power}", k.name) };

    let mut text = Vec::new();
    match (&k.monster, &k.class) {
        (Some(m), _) => text.push(if m.boss { "Boss".to_string() } else { "Monster".to_string() }),
        (None, Some(class)) => text.push(class.clone()),
        (None, None) => {}
    }
    if let Some(cp) = cp {
        let vs = match k.combat_power_vs_me {
            Some(p) if p > 0 => format!(", {p}% above yours"),
            Some(p) if p < 0 => format!(", {}% below yours", -p),
            Some(_) => ", the same as yours".to_string(),
            None => String::new(),
        };
        text.push(format!("{} combat power{vs}", thousands(cp)));
    }
    let blow = view.final_blow.as_ref();
    if let Some(b) = blow {
        text.push(format!("{} {}{}", b.skill.name, thousands(b.damage), if b.crit { " crit" } else { "" }));
    }
    let icon = match (&k.class, blow.and_then(|b| b.skill.icon.clone())) {
        (Some(class), _) if k.player => NoteIcon::Class(class.clone()),
        (_, Some(url)) => NoteIcon::Url(url),
        _ => NoteIcon::Addon,
    };
    Some(DeathNote { title, text: text.join(" · "), icon })
}

/// The hub's log line for a recap shown: which rule opened it and the HP that
/// said so, with no one's name. *Recap: died · HP 22,189 → 0 · 3 hits from 2 ·
/// final blow by a monster*.
pub fn why(fight: &Fight) -> String {
    let rule = match fight.outcome {
        Outcome::Died => "died",
        Outcome::DuelLost => "duel lost",
        Outcome::Survived => "fight over (Show after every fight is on)",
    };
    let mut parts = vec![format!("Recap: {rule}")];
    let hp = |v: Option<i32>| v.map_or("?".to_string(), |v| thousands(i64::from(v)));
    parts.push(format!("HP {} → {}", hp(fight.hp_before), hp(fight.lowest_hp)));
    parts.push(format!("{} hits from {}", fight.hits.len(), fight.attackers.len()));
    if let Some(blow) = fight.final_blow() {
        let player = fight.attackers.iter().any(|a| a.id == blow.actor && a.player);
        parts.push(format!("final blow by a {}", if player { "player" } else { "monster" }));
    }
    parts.join(" · ")
}

/// 140596 → `140.6K`, as the overlay writes it.
fn short(n: i64) -> String {
    if n >= 1000 { format!("{:.1}K", n as f64 / 1000.0) } else { n.to_string() }
}

/// 140596 → `140,596`.
fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// The words beside Death Recap in the hub.
pub fn note(fights: usize) -> String {
    match fights {
        1 => "1 fight recorded".into(),
        n => format!("{n} fights recorded"),
    }
}

/// Every setting with its value now, for the hub.
fn describe(shared: &Shared, client: &mut Client) {
    let groups = lock(&shared.settings).describe();
    let _ = client.set_settings(&groups);
}

fn set_status(shared: &Shared, screen: &dyn Screen, text: &str) {
    let changed = {
        let mut status = lock(&shared.status);
        let changed = *status != text;
        if changed {
            *status = text.to_string();
        }
        changed
    };
    if changed {
        screen.status(text);
    }
}

/// One line to the hub's log (Death Recap's standard output). When the hub
/// is gone the pipe is closed and the line is dropped: never a panic, which
/// would end the reader for good.
pub fn say(out: &mut dyn std::io::Write, text: &str) {
    let _ = writeln!(out, "{text}");
}

/// One event: the player's own numbers, back on their feet, then the recorder.
fn step(recorder: &mut Recorder, reading: &mut Reading, screen: &dyn Screen, e: &Event) -> Vec<Fight> {
    match &e.kind {
        EventKind::MyCombatPower { combat_power, .. } => reading.me.combat_power = Some(*combat_power),
        EventKind::MyGearScore { gear_score, .. } => reading.me.gear_score = Some(*gear_score),
        EventKind::MyServer { server } => reading.my_server = Some(*server),
        EventKind::PlayerServer { name, server } => {
            // Bounded: the game says it again when they come into view.
            if reading.server_of.len() >= 20_000 && !reading.server_of.contains_key(name) {
                reading.server_of.clear();
            }
            reading.server_of.insert(name.clone(), *server);
        }
        EventKind::Hp { id, hp, .. } if reading.dead && Some(*id) == recorder.me() && *hp > 0 => {
            reading.dead = false;
            reading.back_up = true;
            screen.revived();
        }
        _ => {}
    }
    recorder.push(e)
}

/// A fight ended: who killed you into the history, the view to the screen,
/// and, when the player turned them on, the lookups on the official site.
fn ended(
    shared: &Shared,
    screen: &dyn Screen,
    lookups: &Sender<Request>,
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
    // Why it opened, in the hub's log: a bug report then says which rule
    // fired, with no one's name.
    let _ = client.log(LogLevel::Info, &why(&fight));
    let recap = Recap { fight_id: fight.end_ms, view, history, extras: HashMap::new() };
    *lock(&shared.last) = Some(recap.clone());
    if fight.outcome == Outcome::Died {
        reading.dead = true;
    }
    screen.recap(&recap);

    // A note in the hub and on the dock's skull: an alert that names the
    // killer and their power, with their class emblem (a monster: the final
    // blow's icon), taken back when the player is up again or the recap is
    // closed. One per fight: a new death replaces the last one's.
    if let Some(n) = death_note(&recap.view) {
        if let Some(old) = reading.note.take()
            && old != n.title
        {
            let _ = client.unnotify(Some(&old));
        }
        let _ = client.notify(NoteLevel::Alert, &n.title, Some(&n.text), Some(n.icon));
        reading.note = Some(n.title);
        // A lost duel's recap goes by itself and the player never died: its
        // note comes down with the recap (`closed`).
    }

    if lock(&shared.settings).online_lookups {
        let players = recap
            .view
            .by_importance()
            .into_iter()
            .filter(|a| a.player && !a.name.starts_with('#'))
            .map(|a| Player {
                id: a.id,
                name: a.name.clone(),
                server: a.server.or_else(|| reading.server_of.get(&a.name).copied()),
            })
            .collect();
        let region = reading.my_server.and_then(armory::server).map(|s| s.region);
        let _ = lookups.send(Request { fight_id: recap.fight_id, players, region });
    }
}

/// The lookups' worker: what it finds goes into the last recap and to the
/// screen as it comes.
fn start_lookups(shared: &Arc<Shared>, screen: &Arc<dyn Screen>) -> Sender<Request> {
    let (tx, rx) = channel();
    let (shared, screen) = (shared.clone(), screen.clone());
    let _ = std::thread::Builder::new().name("recap-lookups".into()).spawn(move || {
        let allowed = || lock(&shared.settings).online_lookups;
        lookups::run(&shared.armory, rx, Patience::default(), &allowed, &|f: Found| {
            if let Some(page) = f.page {
                // Only the fight being looked up keeps its pages.
                let mut pages = lock(&shared.pages);
                pages.retain(|(fight, _), _| *fight == f.fight_id);
                pages.insert((f.fight_id, f.id), page);
            }
            if let Some(last) = lock(&shared.last).as_mut()
                && last.fight_id == f.fight_id
            {
                last.extras.insert(f.id, f.extra.clone());
            }
            screen.extra(f.fight_id, f.id, &f.extra);
        });
    });
    tx
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A closed pipe (the hub gone) is not a panic.
    #[test]
    fn a_line_to_a_closed_pipe_is_dropped() {
        struct Closed;
        impl std::io::Write for Closed {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
        }
        say(&mut Closed, "Flav-Toolkit went away");
        let mut ok = Vec::new();
        say(&mut ok, "Connected");
        assert_eq!(ok, b"Connected\n");
    }

    /// The hub's log says which rule opened a recap, with no one's name.
    #[test]
    fn the_log_says_why_a_recap_opened() {
        use flav_events::event::HpSource;
        let ev = |at_ms, kind| Event { at_ms, kind };
        let hit = |at_ms, damage| {
            ev(
                at_ms,
                EventKind::Hit {
                    actor: 7,
                    target: 1,
                    skill: 1_800_000,
                    damage,
                    absorbed_by: vec![],
                    multi_hits: 0,
                    dot: false,
                    hit_type: 2,
                    flags: vec![],
                    specs: vec![],
                    class: None,
                    player_skill: false,
                    verified: true,
                    skill_name: None,
                    skill_icon: None,
                },
            )
        };
        let hp = |at_ms, hp| ev(at_ms, EventKind::Hp { id: 1, hp, source: HpSource::Combat });
        let mut r = Recorder::new();
        let mut fights = Vec::new();
        for e in [ev(0, EventKind::Me { id: 1, name: None }), hp(1, 22_189), hit(10, 22_189), hp(10, 0)] {
            fights.extend(r.push(&e));
        }
        fights.extend(r.finish());
        assert_eq!(why(&fights[0]), "Recap: died · HP 22,189 → 0 · 1 hits from 1 · final blow by a monster");
        fights[0].outcome = Outcome::Survived;
        assert_eq!(why(&fights[0]), "Recap: fight over (Show after every fight is on) · HP 22,189 → 0 · 1 hits from 1");
    }
}
