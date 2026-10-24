//! What the Death Recap overlay shows for one fight: a `Fight` turned into
//! plain data with names, the game's icons, and comparisons with the player.
//!
//! The players who hit you are the attackers. Monsters that were hitting you
//! in the same fight (you were farming when the players came) are summed up
//! in `mobs`, unless no player took part, when the monsters are the attackers.

use std::collections::HashMap;

use flav_events::event::{Control, MissKind};
use serde::{Deserialize, Serialize};

use crate::{Fight, Outcome};

/// Names the view needs: skills, entities, NPCs. [`crate::Book`] answers
/// them from what the events said; tests answer them by hand.
pub trait Names {
    fn skill(&self, skill: i32) -> Option<String>;
    /// A skill's icon: `None` when nothing said anything about the skill,
    /// `Some(None)` when it is known and has no icon (a basic attack).
    fn skill_icon(&self, _skill: i32) -> Option<Option<String>> {
        None
    }
    fn entity(&self, id: i32) -> Option<String>;
    fn npc(&self, npc: i32) -> Option<String>;
    /// Whether the game data marks an NPC as a boss.
    fn boss(&self, _npc: i32) -> bool {
        false
    }
}

/// The two-letter code the game's icon files use for a class (`RA` for
/// Ranger): the overlay's class emblems are named by it.
pub fn class_code(class: &str) -> Option<&'static str> {
    Some(match class {
        "Gladiator" => "GL",
        "Templar" => "TE",
        "Assassin" => "AS",
        "Ranger" => "RA",
        "Sorcerer" => "SO",
        "Spiritmaster" | "Elementalist" => "EL",
        "Cleric" => "CL",
        "Chanter" => "CH",
        "Brawler" | "Fighter" => "GT",
        _ => return None,
    })
}

/// The player's own numbers, to compare the attackers with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Me {
    pub combat_power: Option<i64>,
    pub gear_score: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecapView {
    pub outcome: Outcome,
    /// When the fight ended (the death), Unix ms.
    pub at_ms: i64,
    /// From the first player's hit (or the first hit, with no player) to the end.
    pub duration_ms: i64,
    pub damage: i64,
    /// The share of the damage that came from critical hits, 0 to 100.
    pub crit_share: u8,
    /// How long you could do nothing, overlaps counted once.
    pub cc_ms: i64,
    /// How many crowd-control effects landed on you.
    pub cc_count: usize,
    pub me: Me,
    pub final_blow: Option<FinalBlow>,
    pub attackers: Vec<AttackerView>,
    pub mobs: Option<Mobs>,
    /// Skills of theirs you dodged or resisted.
    pub avoided: Vec<Avoided>,
    /// Hits that landed after the end (skills on their way, damage over time).
    pub after_end_damage: i64,
    /// What you dealt in the fight, to everyone.
    pub you_dealt: i64,
    pub you_hits: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FinalBlow {
    pub actor: i32,
    pub skill: SkillRef,
    pub damage: i64,
    pub crit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillRef {
    pub id: i32,
    pub name: String,
    /// The game's icon; `None` for a basic attack.
    pub icon: Option<String>,
    /// `skill`, `passive` (a passive that triggered), `theostone` (a weapon's
    /// Theostone effect), or `attack` (a basic attack).
    pub kind: &'static str,
}

/// What kind of thing a skill id is, from how the game numbers them, and
/// whether the events gave it an icon (a basic attack has none).
fn skill_kind(id: i32, icon: Option<&Option<String>>) -> &'static str {
    let code = format!("{:0<8}", id.unsigned_abs());
    if code.starts_with("30") {
        "theostone"
    } else if matches!(icon, Some(None)) {
        "attack"
    } else if code[2..4].parse::<u32>().is_ok_and(|sub| sub >= 70) {
        "passive"
    } else {
        "skill"
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AttackerView {
    pub id: i32,
    pub name: String,
    pub player: bool,
    pub class: Option<String>,
    /// The two-letter class code of the game's icons (`RA`).
    pub class_code: Option<String>,
    pub server: Option<u16>,
    pub legion: Option<String>,
    /// As the game said when they came into view.
    pub combat_power: Option<i64>,
    /// Their combat power against yours, in percent (−27 = 27% below you).
    pub combat_power_vs_me: Option<i32>,
    /// Filled in later from the official site (online lookups).
    pub gear_score: Option<i32>,
    pub damage: i64,
    pub hits: usize,
    pub crits: usize,
    pub crit_share: u8,
    /// When they joined, from the first player's hit (0 for the one who opened).
    pub joined_ms: Option<i64>,
    pub opener: Option<SkillRef>,
    pub killer: bool,
    /// Times this player killed you in the history kept (this one included).
    pub kills_on_you: usize,
    /// Their skills on you, most damage first; a skill's damage-over-time
    /// ticks are counted with it.
    pub skills: Vec<SkillView>,
    /// Crowd control they put on you, longest first.
    pub cc: Vec<EffectView>,
    /// Other debuffs they put on you.
    pub debuffs: Vec<EffectView>,
    pub cc_ms: i64,
    /// A monster (not a player): `boss` when the game data marks it one.
    /// The game has no portrait for monsters that Flav-Toolkit could show.
    pub monster: Option<Monster>,
    /// What you dealt to them in this fight.
    pub you_dealt: i64,
    pub you_hits: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Monster {
    /// Its NPC code, when its spawn was seen.
    pub npc: Option<i32>,
    pub boss: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillView {
    pub skill: SkillRef,
    pub damage: i64,
    pub hits: usize,
    pub crits: usize,
    /// The specialties chosen for it (slots 1 to 5).
    pub specs: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectView {
    pub abnormal: u32,
    pub name: String,
    pub icon: Option<String>,
    pub control: Option<Control>,
    pub count: usize,
    pub total_ms: i64,
    /// The ability that put it on you (a skill, a passive, a Theostone…);
    /// empty when the game did not say.
    pub sources: Vec<SkillRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Mobs {
    pub damage: i64,
    pub hits: usize,
    /// How long they had been hitting you before the first player did.
    pub before_players_ms: Option<i64>,
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Avoided {
    pub actor: i32,
    pub skill: SkillRef,
    pub resisted: bool,
}

fn skill_ref(names: &dyn Names, id: i32) -> SkillRef {
    let icon = names.skill_icon(id);
    SkillRef {
        id,
        name: names.skill(id).unwrap_or_else(|| format!("Skill {id}")),
        kind: skill_kind(id, icon.as_ref()),
        icon: icon.flatten(),
    }
}

/// The ability that put a debuff on the player, as the buff event (or a hit
/// with it) named it. From a core that does not name a buff's skill (older
/// than its PR #23), a skill no hit or miss named is not in the book: a
/// debuff that is not crowd control is then named after it, as such marks
/// are (Defense Break from Defense Break), and a crowd control from an
/// unnamed skill keeps no source rather than a number.
fn source_ref(names: &dyn Names, skill: i32, d: &crate::Debuff) -> Option<SkillRef> {
    if names.skill(skill).is_some() {
        return Some(skill_ref(names, skill));
    }
    if d.control.is_some() {
        return None;
    }
    let name = d.name.clone()?;
    Some(SkillRef { id: skill, name, kind: skill_kind(skill, None), icon: d.icon.clone() })
}

fn share(part: i64, whole: i64) -> u8 {
    if whole <= 0 { 0 } else { ((part.saturating_mul(100).saturating_add(whole / 2)) / whole).clamp(0, 100) as u8 }
}

impl RecapView {
    /// The attackers in the order they matter most: who killed you, then who
    /// opened the fight (the earliest to join), then the rest by damage. The
    /// order the lookups go in and the faces on the bar show.
    pub fn by_importance(&self) -> Vec<&AttackerView> {
        let killer = self.attackers.iter().position(|a| a.killer);
        let opener = self
            .attackers
            .iter()
            .enumerate()
            .filter(|(i, a)| Some(*i) != killer && a.joined_ms.is_some())
            .min_by_key(|(i, a)| (a.joined_ms, *i))
            .map(|(i, _)| i);
        let first = [killer, opener].into_iter().flatten();
        let rest = (0..self.attackers.len()).filter(|i| Some(*i) != killer && Some(*i) != opener);
        first.chain(rest).map(|i| &self.attackers[i]).collect()
    }

    /// The view of a fight. `kills` counts this player's kills on you in the
    /// history kept (`KillLog::count`).
    pub fn build(fight: &Fight, names: &dyn Names, me: Me, kills: &dyn Fn(&str) -> usize) -> RecapView {
        let players: Vec<i32> = fight.attackers.iter().filter(|a| a.player).map(|a| a.id).collect();
        let any_player = !players.is_empty();
        let first_player_hit = fight
            .hits
            .iter()
            .filter(|h| players.contains(&h.actor))
            .map(|h| h.at_ms)
            .min()
            .or_else(|| fight.debuffs.iter().filter(|d| players.contains(&d.caster)).map(|d| d.from_ms).min());
        let start = if any_player { first_player_hit.unwrap_or(fight.start_ms) } else { fight.start_ms };
        let crit_damage: i64 = fight.hits.iter().filter(|h| h.crit).map(|h| h.damage).sum();
        let final_blow = fight.final_blow().map(|h| FinalBlow {
            actor: h.actor,
            skill: skill_ref(names, h.skill),
            damage: h.damage,
            crit: h.crit,
        });
        let killer = final_blow.as_ref().map(|f| f.actor);

        let mut attackers: Vec<AttackerView> = fight
            .attackers
            .iter()
            .filter(|a| a.player || !any_player)
            .map(|a| {
                let name = a
                    .name
                    .clone()
                    .or_else(|| names.entity(a.id))
                    .or_else(|| a.npc.and_then(|n| names.npc(n)))
                    .unwrap_or_else(|| format!("#{}", a.id));
                let mut skills: Vec<SkillView> = Vec::new();
                for s in &a.skills {
                    let r = skill_ref(names, s.skill);
                    match skills.iter_mut().find(|v| v.skill.name == r.name) {
                        Some(v) => {
                            v.damage += s.damage;
                            v.hits += s.hits;
                            v.crits += s.crits;
                            if v.specs.is_empty() {
                                v.specs = s.specs.clone();
                            }
                        }
                        None => skills.push(SkillView {
                            skill: r,
                            damage: s.damage,
                            hits: s.hits,
                            crits: s.crits,
                            specs: s.specs.clone(),
                        }),
                    }
                }
                skills.sort_by_key(|s| -s.damage);
                let (cc, debuffs) = effects(fight, a.id, names);
                let crit_damage: i64 = fight.hits.iter().filter(|h| h.actor == a.id && h.crit).map(|h| h.damage).sum();
                AttackerView {
                    id: a.id,
                    player: a.player,
                    class_code: a.class.as_deref().and_then(class_code).map(str::to_string),
                    class: a.class.clone(),
                    server: a.server,
                    legion: a.legion.clone(),
                    combat_power: a.combat_power,
                    combat_power_vs_me: match (a.combat_power, me.combat_power) {
                        (Some(theirs), Some(mine)) if mine > 0 => {
                            Some((((theirs - mine) as f64 / mine as f64) * 100.0).round() as i32)
                        }
                        _ => None,
                    },
                    gear_score: None,
                    damage: a.damage,
                    hits: a.hits,
                    crits: a.crits,
                    crit_share: share(crit_damage, a.damage),
                    joined_ms: a.first_hit_ms.map(|t| (t - start).max(0)),
                    opener: a.first_skill.map(|s| skill_ref(names, s)),
                    killer: killer == Some(a.id),
                    kills_on_you: if a.player { kills(&name) } else { 0 },
                    skills,
                    cc,
                    debuffs,
                    cc_ms: a.hard_controlled_ms,
                    monster: (!a.player).then(|| Monster { npc: a.npc, boss: a.npc.is_some_and(|n| names.boss(n)) }),
                    you_dealt: fight.dealt.iter().find(|d| d.target == a.id).map_or(0, |d| d.damage),
                    you_hits: fight.dealt.iter().find(|d| d.target == a.id).map_or(0, |d| d.hits),
                    name,
                }
            })
            .collect();
        attackers.sort_by_key(|a| (-a.damage, a.joined_ms.unwrap_or(i64::MAX)));

        let mobs = any_player
            .then(|| {
                let mob_ids: Vec<&crate::Attacker> = fight.attackers.iter().filter(|a| !a.player).collect();
                if mob_ids.is_empty() {
                    return None;
                }
                let first_mob = mob_ids.iter().filter_map(|a| a.first_hit_ms).min();
                let mut mob_names: Vec<String> = mob_ids
                    .iter()
                    .filter_map(|a| a.name.clone().or_else(|| a.npc.and_then(|n| names.npc(n))))
                    .collect();
                mob_names.dedup();
                Some(Mobs {
                    damage: mob_ids.iter().map(|a| a.damage).sum(),
                    hits: mob_ids.iter().map(|a| a.hits).sum(),
                    before_players_ms: match (first_mob, first_player_hit) {
                        (Some(m), Some(p)) if p > m => Some(p - m),
                        _ => None,
                    },
                    names: mob_names,
                })
            })
            .flatten();

        let avoided = fight
            .avoided
            .iter()
            .filter(|m| players.contains(&m.actor) || !any_player)
            .map(|m| Avoided { actor: m.actor, skill: skill_ref(names, m.skill), resisted: m.kind == MissKind::Resist })
            .collect();

        RecapView {
            outcome: fight.outcome,
            at_ms: fight.end_ms,
            duration_ms: (fight.end_ms - start).max(0),
            damage: fight.damage(),
            crit_share: share(crit_damage, fight.damage()),
            cc_ms: fight.hard_controlled_ms,
            cc_count: fight.debuffs.iter().filter(|d| d.control.is_some()).count(),
            me,
            final_blow,
            attackers,
            mobs,
            avoided,
            after_end_damage: fight.after_end.iter().map(|h| h.damage).sum(),
            you_dealt: fight.dealt.iter().map(|d| d.damage).sum(),
            you_hits: fight.dealt.iter().map(|d| d.hits).sum(),
        }
    }
}

/// One attacker's effects on you, one entry per effect and the ability that
/// put it (a stun from Shadowstrike and a stun from a Theostone are two):
/// crowd control (longest first), and the rest. Effects the game data has no
/// name for (a skill's internal steps) are left out.
fn effects(fight: &Fight, caster: i32, names: &dyn Names) -> (Vec<EffectView>, Vec<EffectView>) {
    let mut by: Vec<EffectView> = Vec::new();
    for d in fight.debuffs.iter().filter(|d| d.caster == caster) {
        let Some(name) = d.name.clone() else { continue };
        let len = d.length_ms().unwrap_or(0).max(0);
        let source = d.skill.and_then(|s| source_ref(names, s as i32, d));
        // The same ability at another level or with other specialties has
        // another id but the same name: one entry.
        let source_name = source.as_ref().map(|s| s.name.clone());
        match by.iter_mut().find(|e| e.name == name && e.sources.first().map(|s| s.name.clone()) == source_name) {
            Some(e) => {
                e.count += 1;
                e.total_ms += len;
            }
            None => by.push(EffectView {
                abnormal: d.abnormal,
                icon: d.icon.clone(),
                control: d.control,
                count: 1,
                total_ms: len,
                sources: source.into_iter().collect(),
                name,
            }),
        }
    }
    let (mut cc, debuffs): (Vec<EffectView>, Vec<EffectView>) = by.into_iter().partition(|e| e.control.is_some());
    cc.sort_by_key(|e| -e.total_ms);
    (cc, debuffs)
}

/// Who killed you, kept for as long as the player chose: this session (in
/// memory only), 24 hours or 3 days.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KillLog {
    pub kills: Vec<Kill>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kill {
    pub at_ms: i64,
    pub killer: String,
    pub server: Option<u16>,
    pub skill: Option<String>,
    pub duel: bool,
}

impl KillLog {
    pub fn add(&mut self, kill: Kill) {
        self.kills.push(kill);
    }

    /// Kills by this player.
    pub fn count(&self, killer: &str) -> usize {
        self.kills.iter().filter(|k| k.killer == killer).count()
    }

    /// Kills by this player, newest first.
    pub fn by(&self, killer: &str) -> Vec<&Kill> {
        let mut v: Vec<&Kill> = self.kills.iter().filter(|k| k.killer == killer).collect();
        v.sort_by_key(|k| -k.at_ms);
        v
    }

    /// Forget kills older than `keep_ms` before `now_ms`.
    pub fn prune(&mut self, now_ms: i64, keep_ms: i64) {
        self.kills.retain(|k| now_ms - k.at_ms <= keep_ms);
    }

    /// Who killed you most, in the history kept.
    pub fn hunters(&self) -> Vec<(String, usize)> {
        let mut m: HashMap<&str, usize> = HashMap::new();
        for k in &self.kills {
            *m.entry(k.killer.as_str()).or_default() += 1;
        }
        let mut v: Vec<(String, usize)> = m.into_iter().map(|(k, n)| (k.to_string(), n)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }
}
