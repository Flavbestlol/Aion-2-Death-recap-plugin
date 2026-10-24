//! Death Recap's logic: every fight on the player, from Flav-Toolkit's events.
//!
//! A fight starts with the first hit, dodge, resist or debuff another entity
//! lands on the player, and ends when the player dies, loses a duel (a duel
//! ends at 1 HP), or no one has touched them for `QUIET_MS`. Each finished
//! fight says who hit the player, with which skills, for how much, and what
//! controlled them (stuns, roots, knockdowns…) and for how long.
//!
//! Feed it every event in order (`Recorder::push`); it hands back each fight
//! as it ends. It reads Flav-Toolkit's events only (`flav-events`, as the
//! feed sends them), so it runs the same on a live feed and on a replayed
//! capture, and needs no game data: names and icons come on the events
//! ([`Book`]).

pub mod book;
pub mod view;

pub use book::Book;

use std::collections::HashMap;

use flav_events::event::{Buff, Control, MissKind};
use flav_events::{Event, EventKind};
use serde::Serialize;

/// A fight ends when no one has hit, missed or debuffed the player for this
/// long.
pub const QUIET_MS: i64 = 15_000;

/// Hits that land this soon after a death or a duel's end belong to that
/// fight (`Fight::after_end`): skills already on their way, damage over time.
/// A fight that ends that way is handed back once this has passed.
pub const AFTER_MS: i64 = 3_000;

/// The player's own hits from this long before a fight's first hit count as
/// part of it (they attacked first).
pub const DEALT_BEFORE_MS: i64 = 10_000;

/// How far back events from before the game says who the player is are kept,
/// to be read once it does.
pub const EARLY_MS: i64 = 300_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Died,
    /// The player's HP stopped at 1 right after a hit: a duel lost.
    DuelLost,
    /// The fight ran out: no one touched the player for `QUIET_MS`.
    Survived,
}

/// One fight on the player.
#[derive(Debug, Clone, Serialize)]
pub struct Fight {
    pub me: i32,
    pub start_ms: i64,
    /// The death, the duel's end, or the last thing that touched the player.
    pub end_ms: i64,
    pub outcome: Outcome,
    /// The player's HP just before the first hit, an estimate: the first HP
    /// record of the fight with the damage before it added back. Within one
    /// payload the game can send the HP record before or after the hit it
    /// follows, so this can be off by a hit.
    pub hp_before: Option<i32>,
    pub lowest_hp: Option<i32>,
    pub max_hp: Option<i32>,
    pub hits: Vec<HitOnMe>,
    /// Dodges and resists.
    pub avoided: Vec<Avoided>,
    /// What others put on the player, crowd control or not.
    pub debuffs: Vec<Debuff>,
    /// Everyone who hit or debuffed the player, most damage first.
    pub attackers: Vec<Attacker>,
    /// How long the player could do nothing (stuns, knockdowns, fear…),
    /// overlaps counted once.
    pub hard_controlled_ms: i64,
    /// Hits that landed within `AFTER_MS` after a death or a duel's end.
    pub after_end: Vec<HitOnMe>,
    /// What the player hit back with, by target, most damage first: from
    /// `DEALT_BEFORE_MS` before the fight's first hit to its end.
    pub dealt: Vec<Dealt>,
}

/// The player's own damage on one target in a fight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dealt {
    pub target: i32,
    pub damage: i64,
    pub hits: usize,
    pub crits: usize,
}

impl Fight {
    /// The last hit, when the fight ended in a death or a lost duel.
    pub fn final_blow(&self) -> Option<&HitOnMe> {
        (self.outcome != Outcome::Survived).then(|| self.hits.last()).flatten()
    }

    pub fn damage(&self) -> i64 {
        self.hits.iter().map(|h| h.damage).sum()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HitOnMe {
    pub at_ms: i64,
    pub actor: i32,
    pub skill: i32,
    pub damage: i64,
    pub crit: bool,
    pub flags: Vec<String>,
    pub dot: bool,
    /// A shield took part of it.
    pub shielded: bool,
    /// The skill's specialties the attacker chose (slots 1 to 5).
    pub specs: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Avoided {
    pub at_ms: i64,
    pub actor: i32,
    pub skill: i32,
    pub kind: MissKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct Debuff {
    pub from_ms: i64,
    /// When it ended (taken off, or ran out); its planned end until then;
    /// `None` when it never ends.
    pub to_ms: Option<i64>,
    pub abnormal: u32,
    pub name: Option<String>,
    /// Its icon on the game's data server (its own, else its skill's), as the
    /// event said.
    pub icon: Option<String>,
    pub control: Option<Control>,
    pub caster: i32,
    pub skill: Option<u32>,
}

impl Debuff {
    /// A debuff from a buff event, starting `at_ms`.
    fn from_buff(b: &Buff, at_ms: i64) -> Debuff {
        Debuff {
            from_ms: at_ms,
            to_ms: b.length_ms.map(|l| at_ms + l),
            abnormal: b.abnormal,
            name: b.name.clone(),
            icon: b.icon.clone(),
            control: control_of(b),
            caster: b.caster,
            skill: b.skill,
        }
    }

    pub fn length_ms(&self) -> Option<i64> {
        self.to_ms.map(|to| to - self.from_ms)
    }
}

/// Everything one entity did to the player in a fight.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Attacker {
    pub id: i32,
    pub name: Option<String>,
    /// The class its skills belong to (players).
    pub class: Option<String>,
    /// It used player skills.
    pub player: bool,
    /// False when every hit came from a monster whose spawn was never seen.
    pub verified: bool,
    pub hits: usize,
    pub crits: usize,
    pub damage: i64,
    /// Per skill, most damage first.
    pub skills: Vec<SkillUse>,
    /// Debuffs it put on the player (crowd control included).
    pub debuffs: usize,
    /// How long its stuns, knockdowns… held the player (overlaps counted once).
    pub hard_controlled_ms: i64,
    /// The player's dodges and resists of its skills.
    pub avoided: usize,
    /// Its first hit on the player in this fight, and with which skill.
    pub first_hit_ms: Option<i64>,
    pub first_skill: Option<i32>,
    /// Its NPC code, for a monster whose spawn said it.
    pub npc: Option<i32>,
    /// From the record that brought it into view (players).
    pub combat_power: Option<i64>,
    pub server: Option<u16>,
    pub legion: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillUse {
    pub skill: i32,
    /// The specialties the attacker chose for it (slots 1 to 5).
    pub specs: Vec<u8>,
    pub hits: usize,
    pub crits: usize,
    pub damage: i64,
    pub biggest: i64,
}

#[derive(Default)]
struct Profile {
    combat_power: Option<i64>,
    server: Option<u16>,
    legion: Option<String>,
}

struct Open {
    start_ms: i64,
    last_ms: i64,
    hp_before: Option<i32>,
    /// The first HP record in the fight, and the damage the fight had done by then.
    first_hp: Option<(i32, i64)>,
    lowest_hp: Option<i32>,
    hits: Vec<HitOnMe>,
    avoided: Vec<Avoided>,
    debuffs: Vec<Debuff>,
    /// Buff instance on the player -> its entry in `debuffs`.
    active: HashMap<i32, usize>,
    /// Per actor: class, player skill, verified (from the hits).
    kinds: HashMap<i32, (Option<String>, bool, bool)>,
}

/// Follows the events and cuts them into fights.
#[derive(Default)]
pub struct Recorder {
    me: Option<i32>,
    hp: Option<i32>,
    max_hp: Option<i32>,
    /// What the events said about who and what they name.
    book: Book,
    profiles: HashMap<i32, Profile>,
    open: Option<Open>,
    /// Events from before the player is known, replayed once they are (a
    /// capture started mid-session learns the player a few seconds in).
    early: Vec<Event>,
    /// The player's own recent hits (target, time, damage, crit), for what
    /// they dealt in a fight.
    outgoing: std::collections::VecDeque<(i32, i64, i64, bool)>,
    /// A fight that ended in a death or a lost duel, kept until `AFTER_MS`
    /// has passed for the hits still on their way.
    ending: Option<(Fight, i64)>,
    /// Healer skills' "hits" on the player waiting for the next HP record,
    /// which tells a heal from a hit (the game sends an ally's heal on you the
    /// way it sends a hit).
    pending: Vec<Event>,
    /// Per skill, how often its "hits" on the player were heals and hits.
    votes: HashMap<i32, (u32, u32)>,
    /// The last HP record of the player: when, and how much it moved.
    last_record: Option<(i64, i32)>,
    /// Damage of the hits put into the fight since that record.
    since_record: i64,
    /// Effects others put on the player that are not crowd control, from the
    /// last moments: a fight that opens takes those of the moment before it.
    recent: Vec<(i32, Debuff)>,
}

/// How long a healer skill's hit on the player waits for an HP record. The
/// game sends the player's HP when it changes; none for this long means it
/// did not (a heal at full HP).
const HP_WAIT_MS: i64 = 3_000;

/// What the player's HP did after hits that may be heals.
#[derive(Clone, Copy)]
enum HpMove {
    /// Moved by this much.
    Moved(i32),
    /// No record came: it did not move.
    Still,
    /// Not known (no HP before, or the events ended).
    Unknown,
}

/// The control a buff takes away, as the event said; from its name when the
/// event carries a name but no control (a core older than the control field).
fn control_of(b: &Buff) -> Option<Control> {
    b.control.or_else(|| b.name.as_deref().and_then(Control::from_name))
}

/// A skill that can heal others: a Cleric's or a Chanter's, other than the
/// basic attack. Its "hit" on the player may be a heal.
fn may_heal(skill: i32) -> bool {
    matches!(skill / 1_000_000, 17 | 18) && skill % 1_000_000 >= 10_000
}

impl Recorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// The player's entity id, once an event said it.
    pub fn me(&self) -> Option<i32> {
        self.me
    }

    /// What the events said so far about the entities and skills they name:
    /// for [`view::RecapView::build`].
    pub fn book(&self) -> &Book {
        &self.book
    }

    /// Read one event. Returns the fights it ended: the one in progress when
    /// this event comes after `QUIET_MS` of nothing, or the one this event
    /// ends (a death, a lost duel).
    pub fn push(&mut self, e: &Event) -> Vec<Fight> {
        self.book.learn(e);
        if self.me.is_none() {
            if let EventKind::Me { id, .. } = e.kind {
                self.me = Some(id as i32);
                let mut done = Vec::new();
                for early in std::mem::take(&mut self.early) {
                    done.extend(self.push(&early));
                }
                done.extend(self.push(e));
                return done;
            }
            // Who is who does not depend on knowing the player: kept now, or
            // a name or combat power seen minutes before the game says who
            // the player is would be lost (an Abyss death, 2026-10-21: the
            // player was known only 100 s in, after the death).
            if matches!(
                e.kind,
                EventKind::Name { .. } | EventKind::PlayerProfile { .. } | EventKind::NpcIdentified { .. }
            ) {
                return self.push_known(e);
            }
            self.early.retain(|old| e.at_ms - old.at_ms <= EARLY_MS);
            self.early.push(e.clone());
            return Vec::new();
        }
        self.push_known(e)
    }

    fn push_known(&mut self, e: &Event) -> Vec<Fight> {
        let mut done = Vec::new();
        if self.pending.first().is_some_and(|p| e.at_ms - p.at_ms > HP_WAIT_MS) {
            self.settle(HpMove::Still);
        }
        if self.ending.as_ref().is_some_and(|(_, until)| e.at_ms > *until)
            && let Some((f, _)) = self.ending.take()
        {
            done.push(f);
        }
        if let Some((f, _)) = &mut self.ending {
            match &e.kind {
                EventKind::Hit { target, actor, .. } if Some(*target) == self.me && Some(*actor) != self.me => {
                    f.after_end.extend(hit_on_me(e));
                    return done;
                }
                EventKind::Missed { target, .. } if Some(*target) == self.me => return done,
                EventKind::BuffAdded(b) if Some(b.entity) == self.me && b.caster != b.entity => return done,
                _ => {}
            }
        }
        if let Some(open) = &self.open
            && e.at_ms - open.last_ms > QUIET_MS
            && let Some(f) = self.close(Outcome::Survived, None)
        {
            done.push(f);
        }
        match &e.kind {
            EventKind::Me { id, .. } => {
                let id = *id as i32;
                if self.me != Some(id) {
                    done.extend(self.ending.take().map(|(f, _)| f));
                    if let Some(f) = self.close(Outcome::Survived, None) {
                        done.push(f);
                    }
                    self.me = Some(id);
                    self.hp = None;
                    self.max_hp = None;
                }
            }
            EventKind::PlayerProfile { id, combat_power, server, legion } => {
                let p = self.profiles.entry(*id).or_default();
                p.combat_power = combat_power.or(p.combat_power);
                p.server = server.or(p.server);
                p.legion = legion.clone().or(p.legion.take());
            }
            EventKind::MaxHp { id, hp } if Some(*id) == self.me => self.max_hp = Some(*hp),
            EventKind::Hit { actor, target, damage, flags, .. }
                if Some(*actor) == self.me && Some(*target) != self.me =>
            {
                self.outgoing.push_back((*target, e.at_ms, *damage, flags.iter().any(|f| f == "critical")));
                while self.outgoing.front().is_some_and(|h| e.at_ms - h.1 > 2 * QUIET_MS + DEALT_BEFORE_MS) {
                    self.outgoing.pop_front();
                }
            }
            EventKind::Hit { actor, target, skill, damage, .. }
                if Some(*target) == self.me && Some(*actor) != self.me =>
            {
                let (skill, damage) = (*skill, *damage);
                let just_before = self.last_record.filter(|(at, _)| e.at_ms - at <= 50).map(|(_, moved)| moved);
                if !may_heal(skill) || self.learned(skill) == Some(false) || self.hostile(*actor) {
                    self.since_record += damage;
                    self.commit(e);
                } else if self.learned(skill) == Some(true) {
                    // A heal.
                } else if let Some(moved) = just_before.filter(|&m| m != 0 && self.pending.is_empty()) {
                    // The HP record came first in the payload: it shows this one.
                    self.vote(skill, moved > 0);
                    if moved < 0 {
                        self.commit(e);
                    }
                } else {
                    self.pending.push(e.clone());
                }
            }
            EventKind::Missed { actor, target, skill, kind, .. }
                if Some(*target) == self.me && Some(*actor) != self.me =>
            {
                let open = self.open_at(e.at_ms);
                open.avoided.push(Avoided { at_ms: e.at_ms, actor: *actor, skill: *skill, kind: *kind });
            }
            EventKind::BuffAdded(b) if Some(b.entity) == self.me && b.caster != b.entity && b.caster != 0 => {
                let d = Debuff::from_buff(b, e.at_ms);
                // Crowd control is hostile: it opens a fight. Anything else
                // (an ally's buff, a mark) joins a fight already going, or one
                // that opens in a moment.
                if d.control.is_some() {
                    let open = self.open_at(e.at_ms);
                    open.debuffs.push(d);
                    open.active.insert(b.instance, open.debuffs.len() - 1);
                } else if let Some(open) = &mut self.open {
                    open.debuffs.push(d);
                    open.active.insert(b.instance, open.debuffs.len() - 1);
                } else {
                    self.recent.retain(|(_, r)| e.at_ms - r.from_ms <= 2_000);
                    self.recent.push((b.instance, d));
                }
            }
            EventKind::BuffChanged(b) if Some(b.entity) == self.me => {
                if let Some(open) = &mut self.open
                    && let Some(&i) = open.active.get(&b.instance)
                {
                    if b.restarted || b.abnormal != open.debuffs[i].abnormal {
                        // Applied again, or the same instance moved on to
                        // another effect (a stun becoming a skill's next
                        // step): the old one ends now, the new one starts.
                        open.debuffs[i].to_ms = Some(e.at_ms);
                        let mut next = Debuff::from_buff(b, e.at_ms);
                        if next.caster == 0 {
                            next.caster = open.debuffs[i].caster;
                        }
                        next.skill = next.skill.or(open.debuffs[i].skill);
                        open.debuffs.push(next);
                        open.active.insert(b.instance, open.debuffs.len() - 1);
                        // Crowd control applied again keeps the fight going;
                        // an ally's buff renewed does not (a party Cleric's
                        // blessing, every few seconds for minutes).
                        if control_of(b).is_some() {
                            open.last_ms = e.at_ms;
                        }
                    } else {
                        // Refreshed: its length now runs from when it began.
                        open.debuffs[i].to_ms = b.length_ms.map(|l| open.debuffs[i].from_ms + l);
                    }
                }
            }
            EventKind::BuffRemoved { entity, removed } if Some(*entity) == self.me => {
                if let Some(open) = &mut self.open {
                    for r in removed {
                        if let Some(i) = open.active.remove(&r.instance) {
                            let d = &mut open.debuffs[i];
                            d.to_ms = Some(d.to_ms.map_or(e.at_ms, |to| to.min(e.at_ms)));
                        }
                    }
                }
            }
            EventKind::Hp { id, hp, .. } if Some(*id) == self.me => {
                let before = self.hp;
                self.settle(before.map_or(HpMove::Unknown, |b| HpMove::Moved(*hp - b)));
                self.last_record = Some((e.at_ms, before.map_or(0, |b| *hp - b)));
                self.since_record = 0;
                self.hp = Some(*hp);
                if let Some(open) = &mut self.open {
                    open.lowest_hp = Some(open.lowest_hp.map_or(*hp, |l| l.min(*hp)));
                    if open.first_hp.is_none() {
                        open.first_hp = Some((*hp, open.hits.iter().map(|h| h.damage).sum()));
                    }
                }
                if *hp == 0 && before.is_some_and(|b| b > 0) {
                    self.open_at(e.at_ms);
                    done.extend(self.end(Outcome::Died, e.at_ms));
                } else if *hp == 1
                    && before.is_some_and(|b| b > 1)
                    && self.open.as_ref().and_then(|o| o.hits.last()).is_some_and(|h| e.at_ms - h.at_ms <= 2_000)
                {
                    done.extend(self.end(Outcome::DuelLost, e.at_ms));
                }
            }
            EventKind::Died { id } if Some(*id) == self.me && self.open.is_some() => {
                done.extend(self.end(Outcome::Died, e.at_ms));
            }
            _ => {}
        }
        done
    }

    /// Fill in what was learned about the attackers after a fight ended
    /// (a name or a combat power that came later in the capture).
    pub fn fill_in(&self, fight: &mut Fight) {
        for a in &mut fight.attackers {
            if a.name.is_none() {
                a.name = self.book.name(a.id).map(str::to_string);
            }
            a.npc = a.npc.or_else(|| self.book.npc(a.id).map(|n| n.code));
            if let Some(p) = self.profiles.get(&a.id) {
                a.combat_power = a.combat_power.or(p.combat_power);
                a.server = a.server.or(p.server);
                a.legion = a.legion.clone().or(p.legion.clone());
                a.player = true;
            }
        }
    }

    /// The time is now `now_ms` and no event came: hand back the fights that
    /// ended by time (after `QUIET_MS` of nothing, or `AFTER_MS` after a death
    /// or a lost duel). For a live feed, which can go quiet.
    pub fn tick(&mut self, now_ms: i64) -> Vec<Fight> {
        let mut done = Vec::new();
        if self.pending.first().is_some_and(|p| now_ms - p.at_ms > HP_WAIT_MS) {
            self.settle(HpMove::Still);
        }
        if self.ending.as_ref().is_some_and(|(_, until)| now_ms > *until)
            && let Some((f, _)) = self.ending.take()
        {
            done.push(f);
        }
        if self.open.as_ref().is_some_and(|o| now_ms - o.last_ms > QUIET_MS) {
            done.extend(self.close(Outcome::Survived, None));
        }
        done
    }

    /// The end of the events (a capture's end): the fight in progress, if any.
    pub fn finish(&mut self) -> Vec<Fight> {
        self.settle(HpMove::Unknown);
        let mut done: Vec<Fight> = self.ending.take().map(|(f, _)| f).into_iter().collect();
        done.extend(self.close(Outcome::Survived, None));
        done
    }

    /// End the fight in progress on a death or a lost duel: it is handed back
    /// once `AFTER_MS` has passed. Returns a fight that ended before, if one
    /// was still waiting.
    fn end(&mut self, outcome: Outcome, at_ms: i64) -> Option<Fight> {
        let earlier = self.ending.take().map(|(f, _)| f);
        if let Some(f) = self.close(outcome, Some(at_ms)) {
            self.ending = Some((f, at_ms + AFTER_MS));
        }
        earlier
    }

    /// Decide the waiting healer-skill hits on the player: heals (dropped)
    /// or hits. HP that went up or stayed put says heals; HP that went down
    /// says whichever reading is closer: they all hit (down by them and the
    /// other hits since the last record), or they all healed (down by the
    /// other hits less up to them).
    fn settle(&mut self, hp: HpMove) {
        let pending = std::mem::take(&mut self.pending);
        if pending.is_empty() {
            return;
        }
        let total: i64 = pending.iter().map(|e| hit_parts(e).2).sum();
        let heal = match hp {
            HpMove::Still => Some(true),
            HpMove::Moved(moved) => {
                let moved = i64::from(moved);
                let as_hits = -(self.since_record + total);
                // Heals can be cut short by full HP: anywhere from none of
                // them to all of them.
                let (low, high) = (-self.since_record, -self.since_record + total);
                let off_heals = if moved < low { low - moved } else { (moved - high).max(0) };
                Some(moved >= 0 || off_heals < (moved - as_hits).abs())
            }
            HpMove::Unknown => None,
        };
        for e in &pending {
            let (actor, skill, _) = hit_parts(e);
            match heal {
                Some(h) => {
                    self.vote(skill, h);
                    if !h || self.hostile(actor) {
                        self.commit(e);
                    }
                }
                None => {
                    if self.learned(skill) != Some(true) || self.hostile(actor) {
                        self.commit(e);
                    }
                }
            }
        }
    }

    fn vote(&mut self, skill: i32, heal: bool) {
        let v = self.votes.entry(skill).or_insert((0, 0));
        if heal {
            v.0 += 1;
        } else {
            v.1 += 1;
        }
    }

    /// What a skill was each time it was told apart (twice or more, always
    /// the same): `Some(true)` heals, `Some(false)` hits.
    fn learned(&self, skill: i32) -> Option<bool> {
        match self.votes.get(&skill) {
            Some(&(heals, 0)) if heals >= 2 => Some(true),
            Some(&(0, hits)) if hits >= 2 => Some(false),
            _ => None,
        }
    }

    /// Whether an entity is hostile in the fight going on: it hit the player
    /// with a skill that cannot heal, missed them, or controlled them. A
    /// hostile one's "hits" are never heals.
    fn hostile(&self, actor: i32) -> bool {
        self.open.as_ref().is_some_and(|o| {
            o.hits.iter().any(|h| h.actor == actor && !may_heal(h.skill))
                || o.avoided.iter().any(|m| m.actor == actor)
                || o.debuffs.iter().any(|d| d.caster == actor && d.control.is_some())
        })
    }

    /// A hit on the player, into the fight.
    fn commit(&mut self, e: &Event) {
        let EventKind::Hit { actor, class, player_skill, verified, .. } = &e.kind else { return };
        let open = self.open_at(e.at_ms);
        if let Some(h) = hit_on_me(e) {
            // A hit decided late goes in at its time.
            let at = open.hits.partition_point(|x| x.at_ms <= h.at_ms);
            open.hits.insert(at, h);
        }
        let k = open.kinds.entry(*actor).or_insert((None, false, false));
        k.0 = k.0.take().or(class.clone());
        k.1 |= *player_skill;
        k.2 |= *verified;
    }

    fn open_at(&mut self, at_ms: i64) -> &mut Open {
        if self.open.is_none() {
            let mut open = Open {
                start_ms: at_ms,
                last_ms: at_ms,
                hp_before: self.hp,
                first_hp: None,
                lowest_hp: self.hp,
                hits: Vec::new(),
                avoided: Vec::new(),
                debuffs: Vec::new(),
                active: HashMap::new(),
                kinds: HashMap::new(),
            };
            // Effects put on the player just before (a mark with the opener).
            for (instance, d) in std::mem::take(&mut self.recent) {
                if at_ms - d.from_ms <= 2_000 {
                    open.debuffs.push(d);
                    open.active.insert(instance, open.debuffs.len() - 1);
                }
            }
            self.open = Some(open);
        }
        let open = self.open.as_mut().expect("just opened");
        open.last_ms = open.last_ms.max(at_ms);
        open
    }

    fn close(&mut self, outcome: Outcome, at_ms: Option<i64>) -> Option<Fight> {
        let mut open = self.open.take()?;
        let me = self.me?;
        let end_ms = at_ms.unwrap_or(open.last_ms);
        // Effects that are not crowd control count only from those who were
        // hostile: who hit the player, missed them, or controlled them. An
        // ally's buffs and heals are not part of the fight.
        let hostile: std::collections::HashSet<i32> = open
            .hits
            .iter()
            .map(|h| h.actor)
            .chain(open.avoided.iter().map(|m| m.actor))
            .chain(open.debuffs.iter().filter(|d| d.control.is_some()).map(|d| d.caster))
            .collect();
        open.debuffs.retain(|d| hostile.contains(&d.caster));
        let mut dealt: Vec<Dealt> = Vec::new();
        for &(target, at, damage, crit) in &self.outgoing {
            if at < open.start_ms - DEALT_BEFORE_MS || at > end_ms {
                continue;
            }
            match dealt.iter_mut().find(|d| d.target == target) {
                Some(d) => {
                    d.damage += damage;
                    d.hits += 1;
                    d.crits += crit as usize;
                }
                None => dealt.push(Dealt { target, damage, hits: 1, crits: crit as usize }),
            }
        }
        dealt.sort_by_key(|d| -d.damage);
        let mut by: HashMap<i32, Attacker> = HashMap::new();
        for h in &open.hits {
            let a = by.entry(h.actor).or_insert_with(|| Attacker { id: h.actor, ..Default::default() });
            if a.first_hit_ms.is_none() {
                a.first_hit_ms = Some(h.at_ms);
                a.first_skill = Some(h.skill);
            }
            a.hits += 1;
            a.damage += h.damage;
            a.crits += h.crit as usize;
            match a.skills.iter_mut().find(|s| s.skill == h.skill) {
                Some(s) => {
                    s.hits += 1;
                    s.crits += h.crit as usize;
                    s.damage += h.damage;
                    s.biggest = s.biggest.max(h.damage);
                }
                None => a.skills.push(SkillUse {
                    skill: h.skill,
                    specs: h.specs.clone(),
                    hits: 1,
                    crits: h.crit as usize,
                    damage: h.damage,
                    biggest: h.damage,
                }),
            }
        }
        for m in &open.avoided {
            by.entry(m.actor).or_insert_with(|| Attacker { id: m.actor, ..Default::default() }).avoided += 1;
        }
        for d in &open.debuffs {
            by.entry(d.caster).or_insert_with(|| Attacker { id: d.caster, ..Default::default() }).debuffs += 1;
        }
        // Control that outlasts a death or a duel's end stops counting there.
        let cut = if outcome == Outcome::Survived { i64::MAX } else { end_ms };
        let hard = |caster: Option<i32>| {
            union_ms(
                open.debuffs
                    .iter()
                    .filter(|d| caster.is_none_or(|c| d.caster == c))
                    .filter(|d| d.control.is_some_and(Control::is_hard))
                    .map(|d| (d.from_ms, d.to_ms.unwrap_or(end_ms).min(cut))),
            )
        };
        let mut attackers: Vec<Attacker> = by
            .into_values()
            .map(|mut a| {
                let (class, player, verified) = open.kinds.get(&a.id).cloned().unwrap_or((None, false, true));
                a.class = class;
                a.player = player || a.class.is_some();
                a.verified = verified || a.hits == 0;
                a.name = self.book.name(a.id).map(str::to_string);
                a.npc = self.book.npc(a.id).map(|n| n.code);
                if let Some(p) = self.profiles.get(&a.id) {
                    a.combat_power = p.combat_power;
                    a.server = p.server;
                    a.legion = p.legion.clone();
                    a.player = true;
                }
                a.skills.sort_by_key(|s| -s.damage);
                a.hard_controlled_ms = hard(Some(a.id));
                a
            })
            .collect();
        attackers.sort_by_key(|a| (-a.damage, -a.hard_controlled_ms, a.id));
        Some(Fight {
            me,
            start_ms: open.start_ms,
            end_ms,
            outcome,
            // The HP the first record in the fight shows, with the damage done
            // before it added back: the last HP seen before the fight can be
            // old (regeneration is not always sent).
            hp_before: open
                .first_hp
                .map(|(hp, dmg)| hp + dmg as i32)
                .map(|hp| self.max_hp.map_or(hp, |max| hp.min(max)))
                .or(open.hp_before),
            lowest_hp: open.lowest_hp,
            max_hp: self.max_hp,
            hard_controlled_ms: hard(None),
            hits: open.hits,
            avoided: open.avoided,
            debuffs: open.debuffs,
            attackers,
            after_end: Vec::new(),
            dealt,
        })
    }
}

/// A hit's actor, skill and damage.
fn hit_parts(e: &Event) -> (i32, i32, i64) {
    match &e.kind {
        EventKind::Hit { actor, skill, damage, .. } => (*actor, *skill, *damage),
        _ => (0, 0, 0),
    }
}

/// A hit as the fight keeps it.
fn hit_on_me(e: &Event) -> Option<HitOnMe> {
    let EventKind::Hit { actor, skill, damage, absorbed_by, dot, flags, specs, .. } = &e.kind else { return None };
    Some(HitOnMe {
        at_ms: e.at_ms,
        actor: *actor,
        skill: *skill,
        damage: *damage,
        crit: flags.iter().any(|f| f == "critical"),
        flags: flags.clone(),
        dot: *dot,
        shielded: !absorbed_by.is_empty(),
        specs: specs.clone(),
    })
}

/// The total length of time spans, overlaps counted once.
fn union_ms(spans: impl Iterator<Item = (i64, i64)>) -> i64 {
    let mut spans: Vec<(i64, i64)> = spans.filter(|(a, b)| b > a).collect();
    spans.sort();
    let mut total = 0;
    let mut cur: Option<(i64, i64)> = None;
    for (a, b) in spans {
        match cur {
            Some((ca, cb)) if a <= cb => cur = Some((ca, cb.max(b))),
            Some((ca, cb)) => {
                total += cb - ca;
                cur = Some((a, b));
            }
            None => cur = Some((a, b)),
        }
    }
    total + cur.map_or(0, |(a, b)| b - a)
}
