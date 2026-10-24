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
/// them half of what is known is forgotten (ids are reused anyway).
const MAX_ENTITIES: usize = 50_000;
const MAX_SKILLS: usize = 20_000;

#[derive(Debug, Clone, Default)]
pub struct Book {
    names: HashMap<i32, String>,
    /// By entity id.
    npcs: HashMap<i32, Npc>,
    /// By NPC code: name and boss.
    npc_codes: HashMap<i32, (Option<String>, bool)>,
    /// By skill id as the events carry it.
    skills: HashMap<i32, Skill>,
}

impl Book {
    /// Write down what an event names.
    pub fn learn(&mut self, e: &Event) {
        match &e.kind {
            EventKind::Name { id, name, .. } if !name.is_empty() => {
                bound(&mut self.names, MAX_ENTITIES);
                self.names.insert(*id, name.clone());
            }
            EventKind::NpcIdentified { id, npc, name, boss } => {
                bound(&mut self.npcs, MAX_ENTITIES);
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
        bound(&mut self.skills, MAX_SKILLS);
        let s = self.skills.entry(skill).or_default();
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

fn bound<V>(map: &mut HashMap<i32, V>, max: usize) {
    if map.len() >= max {
        let mut keys: Vec<i32> = map.keys().copied().collect();
        keys.sort_unstable();
        for k in keys.into_iter().take(max / 2) {
            map.remove(&k);
        }
    }
}
