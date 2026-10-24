//! What the events say about the things they name: who an entity is, which
//! monster an NPC is, what a skill is called and its icon.
//!
//! Flav-Toolkit puts the game data's names and icons on its events (a hit
//! carries its skill's name and icon, a buff the skill that applied it,
//! `npc_identified` the monster's name and whether it is a boss) and sends
//! names as `name` events, so Death Recap
//! keeps no tables of the game data: it writes down what the events say.

use std::collections::HashMap;

use flav_events::{Event, EventKind};

/// A monster or NPC, as `npc_identified` said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Npc {
    /// Its NPC code in the game data.
    pub code: i32,
    pub name: Option<String>,
    pub boss: bool,
}

/// A skill, as a hit or a miss with it said.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Skill {
    pub name: Option<String>,
    /// Its icon on the game's data server; `None` for a basic attack.
    pub icon: Option<String>,
}

/// Bounds, so a session of many hours cannot grow these without end: past
/// them the half learned longest ago is forgotten (ids are reused anyway).
const MAX_ENTITIES: usize = 50_000;
const MAX_SKILLS: usize = 20_000;
const MAX_NPC_CODES: usize = 20_000;

#[derive(Debug, Clone)]
pub struct Book {
    names: Bounded<String>,
    /// By entity id.
    npcs: Bounded<Npc>,
    /// By NPC code: name and boss.
    npc_codes: Bounded<(Option<String>, bool)>,
    /// By skill id as the events carry it.
    skills: Bounded<Skill>,
}

impl Default for Book {
    fn default() -> Self {
        Book {
            names: Bounded::new(MAX_ENTITIES),
            npcs: Bounded::new(MAX_ENTITIES),
            npc_codes: Bounded::new(MAX_NPC_CODES),
            skills: Bounded::new(MAX_SKILLS),
        }
    }
}

impl Book {
    /// Write down what an event names.
    pub fn learn(&mut self, e: &Event) {
        match &e.kind {
            EventKind::Name { id, name, .. } if !name.is_empty() => {
                self.names.insert(*id, name.clone());
            }
            EventKind::NpcIdentified { id, npc, name, boss } => {
                self.npcs.insert(*id, Npc { code: *npc, name: name.clone(), boss: *boss });
                if name.is_some() || *boss {
                    self.npc_codes.insert(*npc, (name.clone(), *boss));
                }
            }
            // A buff names the skill that applied it (a passive that only
            // marks, a stun that does no damage: skills no hit names).
            EventKind::BuffAdded(b) | EventKind::BuffChanged(b) => self.learn_buff(b),
            EventKind::BuffsListed { buffs, .. } => buffs.iter().for_each(|b| self.learn_buff(b)),
            EventKind::Hit { skill, skill_name, skill_icon, .. }
            | EventKind::Missed { skill, skill_name, skill_icon, .. }
            | EventKind::Heal { skill, skill_name, skill_icon, .. }
                if skill_name.is_some() || skill_icon.is_some() =>
            {
                self.learn_skill(*skill, skill_name, skill_icon);
            }
            _ => {}
        }
    }

    fn learn_buff(&mut self, b: &flav_events::event::Buff) {
        if let Some(skill) = b.skill
            && (b.skill_name.is_some() || b.skill_icon.is_some())
        {
            self.learn_skill(skill as i32, &b.skill_name, &b.skill_icon);
        }
    }

    fn learn_skill(&mut self, skill: i32, name: &Option<String>, icon: &Option<String>) {
        let s = self.skills.entry(skill);
        s.name = name.clone().or(s.name.take());
        s.icon = icon.clone().or(s.icon.take());
    }

    /// An entity's name.
    pub fn name(&self, id: i32) -> Option<&str> {
        self.names.get(&id).map(String::as_str)
    }

    /// The NPC an entity is.
    pub fn npc(&self, id: i32) -> Option<&Npc> {
        self.npcs.get(&id)
    }

    /// A skill by its id; a specialised skill (`14110230`) falls back to its
    /// base skill (`14110000`), the id hits carry.
    pub fn skill(&self, id: i32) -> Option<&Skill> {
        self.skills.get(&id).or_else(|| self.skills.get(&(id / 10_000 * 10_000)))
    }
}

impl crate::view::Names for Book {
    fn skill(&self, skill: i32) -> Option<String> {
        Book::skill(self, skill).and_then(|s| s.name.clone())
    }
    fn skill_icon(&self, skill: i32) -> Option<Option<String>> {
        Book::skill(self, skill).map(|s| s.icon.clone())
    }
    fn entity(&self, id: i32) -> Option<String> {
        self.name(id).map(str::to_string)
    }
    fn npc(&self, npc: i32) -> Option<String> {
        self.npc_codes.get(&npc).and_then(|(name, _)| name.clone())
    }
    fn boss(&self, npc: i32) -> bool {
        self.npc_codes.get(&npc).is_some_and(|(_, boss)| *boss)
    }
}

/// A map by id that keeps at most `max` entries: when full, the half
/// learned (or relearned) longest ago is forgotten.
#[derive(Debug, Clone)]
struct Bounded<V> {
    map: HashMap<i32, (u64, V)>,
    next: u64,
    max: usize,
}

impl<V: Default> Bounded<V> {
    fn entry(&mut self, id: i32) -> &mut V {
        if !self.map.contains_key(&id) {
            self.make_room();
        }
        self.next += 1;
        let stamp = self.next;
        let slot = self.map.entry(id).or_insert_with(|| (stamp, V::default()));
        slot.0 = stamp;
        &mut slot.1
    }
}

impl<V> Bounded<V> {
    fn new(max: usize) -> Self {
        Bounded { map: HashMap::new(), next: 0, max: max.max(2) }
    }

    fn insert(&mut self, id: i32, value: V) {
        if !self.map.contains_key(&id) {
            self.make_room();
        }
        self.next += 1;
        self.map.insert(id, (self.next, value));
    }

    fn get(&self, id: &i32) -> Option<&V> {
        self.map.get(id).map(|(_, v)| v)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.map.len()
    }

    fn make_room(&mut self) {
        if self.map.len() < self.max {
            return;
        }
        let mut stamps: Vec<u64> = self.map.values().map(|(stamp, _)| *stamp).collect();
        stamps.sort_unstable();
        let keep_from = stamps[stamps.len() / 2];
        self.map.retain(|_, (stamp, _)| *stamp >= keep_from);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Full: the oldest half goes, the names learned last stay, whatever
    /// their ids.
    #[test]
    fn the_oldest_are_forgotten_first() {
        let mut b: Bounded<String> = Bounded::new(4);
        for id in [900, 800, 700, 600] {
            b.insert(id, id.to_string());
        }
        b.insert(5, "new".into());
        assert!(b.len() <= 4);
        assert_eq!(b.get(&5).map(String::as_str), Some("new"));
        assert_eq!(b.get(&600).map(String::as_str), Some("600"), "learned last, kept");
        assert!(b.get(&900).is_none(), "learned first, forgotten");
    }
}
