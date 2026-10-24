//! Death Recap's fights, from Flav-Toolkit's events.
//!
//! The fixtures are the events the core's engine read from three real
//! sessions (ids only), one JSON line each as the feed sends them, and the
//! views the built-in Death Recap showed for them
//! (`tools/make-fixtures`): this repository's recap must build the same.

use flav_events::event::{Buff, Control, HpSource, MissKind};
use flav_events::{Event, EventKind};
use recap::{Fight, Outcome, QUIET_MS, Recorder};

const DUELS: &str = include_str!("fixtures/duels-2026-10-21.events.jsonl");
const DUEL_LOST: &str = include_str!("fixtures/duel-lost-2026-10-21.events.jsonl");
const ABYSS_LIVE: &str = include_str!("fixtures/abyss-death-live-2026-10-21.events.jsonl");
const FOE: i32 = 16072;

fn events(fixture: &str) -> Vec<Event> {
    fixture.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).expect("an event")).collect()
}

/// Every event through a recorder; the fights, and the recorder (for its book).
fn replay(events: &[Event]) -> (Vec<Fight>, Recorder) {
    let mut r = Recorder::new();
    let mut out = Vec::new();
    for e in events {
        out.extend(r.push(e));
    }
    out.extend(r.finish());
    for f in &mut out {
        r.fill_in(f);
    }
    (out, r)
}

/// As in the capture, the game says who the player is a few seconds in: the
/// first duel's events come before it (the `me` line in the fixture) and must
/// still count.
fn duels() -> Vec<Fight> {
    replay(&events(DUELS)).0
}

/// The views the built-in Death Recap built from the same sessions, field
/// for field (`*.views.json`): names, skills, icons, crowd control, timings.
#[test]
fn the_views_match_the_built_in_recap() {
    use recap::view::{Me, RecapView};
    for (name, fixture, expected) in [
        ("duels", DUELS, include_str!("fixtures/duels-2026-10-21.views.json")),
        ("duel lost", DUEL_LOST, include_str!("fixtures/duel-lost-2026-10-21.views.json")),
        ("abyss death", ABYSS_LIVE, include_str!("fixtures/abyss-death-live-2026-10-21.views.json")),
    ] {
        let (fights, r) = replay(&events(fixture));
        let me = Me { combat_power: Some(168_152), gear_score: Some(2_715) };
        let ours: Vec<serde_json::Value> =
            fights.iter().map(|f| serde_json::to_value(RecapView::build(f, r.book(), me, &|_| 1)).unwrap()).collect();
        let expected: Vec<serde_json::Value> = serde_json::from_str(expected).unwrap();
        assert_eq!(ours, expected, "{name}");
    }
}

/// One Quick Slice, as the meter showed it: Quick Slice twice (240 + 240),
/// Clone Attack 96 and Exploit Weakness 96, all critical, from behind; and
/// two Insignia marks on the player.
#[test]
fn the_first_duel_is_one_quick_slice() {
    let fights = duels();
    assert_eq!(fights.len(), 2, "two duels, 3 minutes apart");
    let f = &fights[0];
    assert_eq!(f.outcome, Outcome::Survived);
    assert_eq!(f.hits.len(), 4);
    assert_eq!(f.damage(), 672);
    assert!(f.hits.iter().all(|h| h.crit && h.flags.contains(&"back".to_string())));
    assert_eq!(f.hp_before, Some(19_173));
    assert_eq!(f.lowest_hp, Some(18_501));
    let a = &f.attackers[0];
    assert_eq!((a.id, a.class.as_deref(), a.player), (FOE, Some("Assassin"), true));
    let skills: Vec<(i32, usize, i64)> = a.skills.iter().map(|s| (s.skill, s.hits, s.damage)).collect();
    assert_eq!(skills[0], (13_010_000, 2, 480), "Quick Slice");
    assert!(skills.contains(&(13_720_005, 1, 96)), "Clone Attack");
    assert!(skills.contains(&(13_720_000, 1, 96)), "Exploit Weakness");
    // Ten seconds each: the removal record came 5 ms before the planned end.
    let marks: Vec<(u32, Option<i64>, Option<Control>)> =
        f.debuffs.iter().map(|d| (d.abnormal, d.length_ms(), d.control)).collect();
    assert_eq!(marks, vec![(130_000_011, Some(9_995), None), (130_000_011, Some(9_995), None)]);
    assert_eq!(f.debuffs[0].name.as_deref(), Some("Insignia"));
    assert_eq!(f.hard_controlled_ms, 0);
}

/// Shadowstrike stuns the player for 3 seconds (it ran out at 3.000 s), and
/// one of its effects was resisted; Ambush Stance then breaks their defense.
/// The meter's own fight view said it saw no buff or debuff in this fight.
#[test]
fn the_second_duel_is_a_three_second_stun() {
    let fights = duels();
    let f = &fights[1];
    assert_eq!(f.outcome, Outcome::Survived);
    assert_eq!(f.damage(), 1_136);
    assert_eq!(
        f.hits.iter().map(|h| (h.skill, h.damage)).collect::<Vec<_>>(),
        vec![(13_070_000, 935), (13_770_000, 201)]
    );
    let stun = f.debuffs.iter().find(|d| d.control.is_some()).expect("the stun");
    assert_eq!(stun.abnormal, 100_000_301);
    assert_eq!(stun.name.as_deref(), Some("Stun"));
    assert_eq!(stun.control, Some(Control::Stun));
    assert_eq!((stun.caster, stun.skill), (FOE, Some(13_070_230)));
    assert_eq!(stun.from_ms, f.hits[0].at_ms, "with the Shadowstrike hit");
    assert_eq!(stun.length_ms(), Some(3_000));
    let defense = f.debuffs.iter().find(|d| d.abnormal == 137_800_071).expect("Defense Break");
    assert_eq!((defense.control, defense.length_ms()), (None, Some(10_000)));
    assert_eq!(f.avoided.len(), 1);
    assert_eq!(f.avoided[0].kind, MissKind::Resist);
    assert_eq!(f.hard_controlled_ms, 3_000);
    assert_eq!(f.attackers[0].hard_controlled_ms, 3_000);
    assert_eq!(f.lowest_hp, Some(18_037));
}

/// A whole duel, lost. The meter's numbers for the same fight: 28.75k damage
/// (its fight also holds the 3 hits after the end), Heart Gore 9.88k in 12,
/// Quick Slice 1.87k in 8, Flash Slice 3.58k in 4, Savage Fang 3.29k in 6,
/// Triniel's Dagger 3.06k in 2, Shadow Fall 1.69k in 2, Determination 1.53k
/// in 3, Ambush Stance 1.30k in 4; and, again, no debuffs seen.
#[test]
fn a_lost_duel_ends_at_one_hp_with_every_hit_and_every_stun() {
    let (fights, _) = replay(&events(DUEL_LOST));
    assert_eq!(fights.len(), 1, "the hits after the end belong to the duel");
    let f = &fights[0];
    assert_eq!(f.outcome, Outcome::DuelLost);
    assert_eq!(f.lowest_hp, Some(1));
    assert_eq!((f.hits.len(), f.damage()), (58, 28_142));
    assert_eq!(f.after_end.iter().map(|h| h.damage).sum::<i64>(), 608);
    assert_eq!(f.damage() + 608, 28_750, "the meter's total");
    let by_skill = |id: i32| -> (usize, i64) {
        let a = &f.attackers[0];
        let s = a.skills.iter().find(|s| s.skill == id).unwrap();
        (s.hits, s.damage)
    };
    assert_eq!(by_skill(13_010_000), (8, 1_866), "Quick Slice");
    assert_eq!(by_skill(13_220_000).1, 1_693, "Shadow Fall");
    let blow = f.final_blow().unwrap();
    assert_eq!((blow.actor, blow.skill, blow.damage, blow.crit), (128, 13_010_000, 104, true));
    // Shadowstrike's stun, until Shadow Fall's next step took its place;
    // then Shadow Fall's knockdown, 3 s.
    let stun = f.debuffs.iter().find(|d| d.abnormal == 100_000_301).unwrap();
    assert_eq!((stun.skill, stun.length_ms()), (Some(13_070_230), Some(2_528)));
    let down = f.debuffs.iter().find(|d| d.abnormal == 100_000_321).unwrap();
    assert_eq!((down.control, down.length_ms()), (Some(Control::Knock), Some(3_000)));
    assert_eq!(f.hard_controlled_ms, 5_528);
    assert_eq!(f.avoided.iter().filter(|m| m.kind == MissKind::Resist).count(), 11);
}

/// The first death read live. The game said who the player was only after
/// it: every hit from before still counts.
#[test]
fn a_death_before_the_player_is_known_is_still_whole() {
    const PLAYER: i32 = 12_445;
    let mut recorder = Recorder::new();
    let mut fights = Vec::new();
    let mut last = 0;
    // The fixture says who the player is first; here the game says it last.
    for e in events(ABYSS_LIVE).iter().filter(|e| !matches!(e.kind, EventKind::Me { .. })) {
        fights.extend(recorder.push(e));
        last = e.at_ms;
    }
    assert!(fights.is_empty(), "the player is not known yet");
    fights.extend(recorder.push(&Event { at_ms: last, kind: EventKind::Me { id: PLAYER as i64, name: None } }));
    fights.extend(recorder.finish());
    assert_eq!(fights.len(), 1);
    let f = &fights[0];
    assert_eq!(f.outcome, Outcome::Died);
    assert_eq!((f.hits.len(), f.damage()), (42, 22_992));
    assert_eq!(f.after_end.iter().map(|h| h.damage).sum::<i64>(), 884);
    let blow = f.final_blow().unwrap();
    assert_eq!((blow.actor, blow.damage), (12_645, 647));
    let ids: Vec<i32> = f.attackers.iter().map(|a| a.id).collect();
    assert_eq!(ids, vec![10_159, 12_645]);
    assert_eq!(f.attackers[0].damage, 17_290);
    assert_eq!(f.avoided.len(), 2, "Snare Shot's snare and a Flash Slice effect, resisted");
}

/// Who is who is kept from the start, however long before the game says
/// who the player is.
#[test]
fn names_and_combat_power_from_long_before_are_kept() {
    let mut r = Recorder::new();
    r.push(&ev(0, EventKind::Name { id: 7, name: "Sampleton".into(), authoritative: true }));
    r.push(&ev(0, EventKind::PlayerProfile { id: 7, combat_power: Some(161_706), server: Some(2304), legion: None }));
    r.push(&ev(100_000, EventKind::Me { id: 1, name: None }));
    let mut out = Vec::new();
    for e in [hp(100_001, 1000), hit(100_010, 7, 1000), hp(100_010, 0)] {
        out.extend(r.push(&e));
    }
    out.extend(r.finish());
    let a = &out[0].attackers[0];
    assert_eq!((a.name.as_deref(), a.combat_power), (Some("Sampleton"), Some(161_706)));
}

fn ev(at_ms: i64, kind: EventKind) -> Event {
    Event { at_ms, kind }
}

fn hit(at_ms: i64, actor: i32, damage: i64) -> Event {
    ev(
        at_ms,
        EventKind::Hit {
            actor,
            target: 1,
            skill: 11_070_000,
            damage,
            absorbed_by: vec![],
            multi_hits: 0,
            dot: false,
            hit_type: 2,
            flags: vec![],
            specs: vec![],
            class: Some("Gladiator".into()),
            player_skill: true,
            verified: true,
            skill_name: None,
            skill_icon: None,
        },
    )
}

fn hp(at_ms: i64, hp: i32) -> Event {
    ev(at_ms, EventKind::Hp { id: 1, hp, source: HpSource::Combat })
}

fn run(events: &[Event]) -> Vec<Fight> {
    let mut r = Recorder::new();
    let mut out = Vec::new();
    for e in events {
        out.extend(r.push(e));
    }
    out.extend(r.finish());
    out
}

fn me() -> Event {
    ev(0, EventKind::Me { id: 1, name: None })
}

#[test]
fn a_death_ends_the_fight_on_the_final_blow() {
    let f = run(&[me(), hp(1, 1000), hit(10, 7, 600), hp(10, 400), hit(20, 8, 500), hp(20, 0)]);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].outcome, Outcome::Died);
    assert_eq!(f[0].final_blow().map(|h| (h.actor, h.damage)), Some((8, 500)));
    assert_eq!(f[0].attackers.iter().map(|a| a.id).collect::<Vec<_>>(), vec![7, 8], "most damage first");
}

#[test]
fn a_duel_ends_at_one_hp() {
    let f = run(&[me(), hp(1, 1000), hit(10, 7, 999), hp(10, 1)]);
    assert_eq!(f[0].outcome, Outcome::DuelLost);
    assert_eq!(f[0].final_blow().map(|h| h.damage), Some(999));
}

#[test]
fn quiet_splits_fights() {
    let f = run(&[me(), hit(10, 7, 5), hit(10 + QUIET_MS, 7, 5), hit(20 + 2 * QUIET_MS + 1, 7, 5)]);
    assert_eq!(f.iter().map(|f| f.hits.len()).collect::<Vec<_>>(), vec![2, 1]);
    assert!(f.iter().all(|f| f.outcome == Outcome::Survived && f.final_blow().is_none()));
}

/// Two stuns that overlap hold the player for their union, not their sum;
/// a stun cut short by death counts until the death.
#[test]
fn control_time_counts_overlaps_once() {
    let stun = |at, instance, caster, len| {
        ev(
            at,
            EventKind::BuffAdded(Buff {
                entity: 1,
                instance,
                abnormal: 100_000_301,
                caster,
                skill: None,
                level: 1,
                length_ms: Some(len),
                end_ms: None,
                restarted: false,
                name: Some("Stun".into()),
                icon: None,
                control: Some(Control::Stun),
                skill_name: None,
                skill_icon: None,
            }),
        )
    };
    let f =
        run(&[me(), hp(1, 1000), stun(100, 1, 7, 3_000), stun(1_100, 2, 8, 3_000), hit(2_000, 7, 1000), hp(2_000, 0)]);
    assert_eq!(f[0].outcome, Outcome::Died);
    assert_eq!(f[0].hard_controlled_ms, 1_900, "from 100 to the death at 2000");
    let by7 = f[0].attackers.iter().find(|a| a.id == 7).unwrap();
    assert_eq!(by7.hard_controlled_ms, 1_900);
    let by8 = f[0].attackers.iter().find(|a| a.id == 8).unwrap();
    assert_eq!((by8.hits, by8.debuffs, by8.hard_controlled_ms), (0, 1, 900));
}

/// Live, events can stop: time alone ends a fight, and a death is handed
/// back once the hits still on their way had their time.
#[test]
fn time_alone_ends_fights() {
    let mut r = Recorder::new();
    for e in [me(), hp(1, 1000), hit(10, 7, 400), hp(10, 600)] {
        assert!(r.push(&e).is_empty());
    }
    assert!(r.tick(10 + QUIET_MS).is_empty());
    let f = r.tick(11 + QUIET_MS);
    assert_eq!((f.len(), f[0].outcome), (1, Outcome::Survived));
    for e in [hit(30_000, 7, 600), hp(30_000, 0)] {
        assert!(r.push(&e).is_empty(), "held for the hits still on their way");
    }
    assert!(r.tick(30_000 + recap::AFTER_MS).is_empty());
    let f = r.tick(30_001 + recap::AFTER_MS);
    assert_eq!((f.len(), f[0].outcome), (1, Outcome::Died));
}

/// The view the overlay shows, for the live Abyss death.
#[test]
fn the_overlay_view_of_the_abyss_death() {
    use recap::view::{Me, RecapView};
    let (fights, recorder) = replay(&events(ABYSS_LIVE));
    let me = Me { combat_power: Some(168_152), gear_score: Some(2_715) };
    let v = RecapView::build(&fights[0], recorder.book(), me, &|_| 1);
    let ids: Vec<i32> = v.attackers.iter().map(|a| a.id).collect();
    assert_eq!(ids, vec![10_159, 12_645]);
    let (r, g) = (&v.attackers[0], &v.attackers[1]);
    assert_eq!((r.joined_ms, g.joined_ms), (Some(0), Some(1_698)), "the Assassin joined 1.7 s after the Ranger opened");
    assert_eq!(r.opener.as_ref().map(|s| s.name.as_str()), Some("Spiral Arrow"));
    assert_eq!(g.opener.as_ref().map(|s| s.name.as_str()), Some("Flash Slice"));
    assert!(g.killer && !r.killer);
    assert_eq!(v.final_blow.as_ref().map(|f| (f.skill.name.as_str(), f.damage)), Some(("Flash Slice", 647)));
    // Two knockbacks, from two abilities: each one says which.
    let cc: Vec<(&str, &str, usize)> =
        r.cc.iter().map(|e| (e.name.as_str(), e.sources[0].name.as_str(), e.count)).collect();
    assert_eq!(cc.len(), 2);
    assert!(cc.contains(&("Knock Back", "Gale Arrow", 1)) && cc.contains(&("Knock Back", "Marking Shot", 1)), "{cc:?}");
    assert!(r.cc.iter().all(|e| e.sources[0].kind == "skill"));
    assert!(r.cc[0].icon.as_deref().is_some_and(|i| i.ends_with("ICON_Abnormal_Move.png")));
    assert_eq!(r.debuffs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), vec!["Bleed"]);
    assert_eq!(r.skills[0].skill.name, "Drill Dart");
    assert!(r.skills[0].skill.icon.as_deref().is_some_and(|i| i.ends_with("ICON_RA_SKILL_005.png")));
    assert_eq!(r.class_code.as_deref(), Some("RA"));
    assert_eq!((v.damage, v.crit_share), (22_992, 67));
    assert_eq!(v.duration_ms, 3_400);
    assert!(v.mobs.is_none());
}

#[test]
fn the_kill_log_counts_and_forgets() {
    use recap::view::{Kill, KillLog};
    let mut log = KillLog::default();
    let kill =
        |at_ms, killer: &str| Kill { at_ms, killer: killer.into(), server: Some(2304), skill: None, duel: false };
    log.add(kill(0, "Testerella"));
    log.add(kill(60_000, "Exampla"));
    log.add(kill(120_000, "Testerella"));
    assert_eq!(log.count("Testerella"), 2);
    assert_eq!(log.by("Testerella")[0].at_ms, 120_000, "newest first");
    assert_eq!(log.hunters()[0], ("Testerella".to_string(), 2));
    log.prune(120_000, 90_000);
    assert_eq!(log.count("Testerella"), 1);
    assert_eq!(log.count("Exampla"), 1);
}

/// A healer skill's "hit" on the player, the way the game sends an ally's
/// heal (Radiant Benediction from a party Cleric, 2026-10-21).
fn heal_hit(at_ms: i64, actor: i32, amount: i64) -> Event {
    let mut e = hit(at_ms, actor, amount);
    if let EventKind::Hit { skill, class, .. } = &mut e.kind {
        *skill = 17_800_000;
        *class = Some("Cleric".into());
    }
    e
}

/// An ally's heals are not hits: at full HP no HP record follows them, below
/// it HP goes up. A hit that comes with one still counts, by the HP: down by
/// the hit and the heal's amount is a hit, down by the hit less it a heal.
#[test]
fn heals_are_not_hits() {
    // At full HP: heals with no HP record, then a monster's hit.
    let f = run(&[
        me(),
        hp(1, 20_000),
        heal_hit(1_000, 9, 1_500),
        heal_hit(6_000, 9, 1_500),
        hit(7_000, 5, 400),
        hp(7_000, 19_600),
    ]);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].attackers.iter().map(|a| a.id).collect::<Vec<_>>(), vec![5]);
    // Below full: HP up by the heal.
    let f = run(&[me(), hp(1, 10_000), hit(100, 5, 400), hp(100, 9_600), heal_hit(1_000, 9, 1_500), hp(1_000, 11_100)]);
    assert_eq!(f[0].damage(), 400);
    // A hit and a heal in the same moment: down by 4000 - 1500.
    let f = run(&[me(), hp(1, 20_000), hit(100, 5, 4_000), heal_hit(100, 9, 1_500), hp(100, 17_500)]);
    assert_eq!(f[0].damage(), 4_000);
    // An enemy Cleric's skill that hurts: down by it.
    let f = run(&[me(), hp(1, 20_000), heal_hit(100, 8, 1_500), hp(100, 18_500), hp(200, 0)]);
    assert_eq!((f[0].outcome, f[0].damage()), (Outcome::Died, 1_500));
}

/// A Cleric that is hostile (it controlled the player) does not heal them:
/// its skills count as hits even with no HP record.
#[test]
fn a_hostile_healer_hits() {
    let stun = ev(
        50,
        EventKind::BuffAdded(Buff {
            entity: 1,
            instance: 1,
            abnormal: 100_000_301,
            caster: 8,
            skill: None,
            level: 1,
            length_ms: Some(1_000),
            end_ms: None,
            restarted: false,
            name: Some("Stun".into()),
            icon: None,
            control: Some(Control::Stun),
            skill_name: None,
            skill_icon: None,
        }),
    );
    let f = run(&[me(), hp(1, 20_000), stun, heal_hit(100, 8, 1_500)]);
    assert_eq!(f[0].damage(), 1_500);
}

/// A monster that kills the player is shown as one: no class, no combat
/// power, and a boss when the game data says so (Watcher Kaira, 2026-10-22,
/// showed a broken portrait).
#[test]
fn a_monster_killer_is_a_monster() {
    use recap::view::{Me, Names, RecapView};
    struct Data;
    impl Names for Data {
        fn skill(&self, _: i32) -> Option<String> {
            None
        }
        fn entity(&self, _: i32) -> Option<String> {
            None
        }
        fn npc(&self, npc: i32) -> Option<String> {
            (npc == 2_600_089).then(|| "Watcher Kaira".into())
        }
        fn boss(&self, npc: i32) -> bool {
            npc == 2_600_089
        }
    }
    let mut boss_hit = hit(100, 7, 9_165);
    if let EventKind::Hit { skill, class, player_skill, .. } = &mut boss_hit.kind {
        *skill = 1_228_230;
        *class = None;
        *player_skill = false;
    }
    let f = run(&[
        me(),
        hp(1, 9_000),
        ev(50, EventKind::NpcIdentified { id: 7, npc: 2_600_089, name: None, boss: false }),
        boss_hit,
        hp(100, 0),
    ]);
    let v = RecapView::build(&f[0], &Data, Me::default(), &|_| 0);
    let a = &v.attackers[0];
    assert_eq!((a.name.as_str(), a.player, a.class_code.as_deref()), ("Watcher Kaira", false, None));
    assert_eq!(a.monster.map(|m| (m.npc, m.boss)), Some((Some(2_600_089), true)));
}

/// Monsters are named from the events alone: `npc_identified` carries the
/// monster's name and whether it is a boss.
#[test]
fn a_monster_is_named_from_the_events() {
    use recap::view::{Me, RecapView};
    let mut boss_hit = hit(100, 7, 9_165);
    if let EventKind::Hit { skill, class, player_skill, skill_name, .. } = &mut boss_hit.kind {
        *skill = 1_228_230;
        *class = None;
        *player_skill = false;
        *skill_name = Some("Sphere Explosion".into());
    }
    let (f, r) = replay(&[
        me(),
        hp(1, 9_000),
        ev(50, EventKind::NpcIdentified { id: 7, npc: 2_600_089, name: Some("Watcher Kaira".into()), boss: true }),
        boss_hit,
        hp(100, 0),
    ]);
    let v = RecapView::build(&f[0], r.book(), Me::default(), &|_| 0);
    let a = &v.attackers[0];
    assert_eq!(a.name, "Watcher Kaira");
    assert_eq!(a.monster.map(|m| (m.npc, m.boss)), Some((Some(2_600_089), true)));
    assert_eq!(v.final_blow.map(|b| b.skill.name), Some("Sphere Explosion".to_string()));
}

/// A debuff from a skill no hit named (a passive that only marks) is named
/// after itself; a crowd control from such a skill keeps no source rather
/// than a number.
#[test]
fn a_debuff_from_an_unnamed_skill() {
    use recap::view::{Me, RecapView};
    let buff = |instance, abnormal, name: &str, control| {
        ev(
            20,
            EventKind::BuffAdded(Buff {
                entity: 1,
                instance,
                abnormal,
                caster: 7,
                skill: Some(13_780_007),
                level: 1,
                length_ms: Some(10_000),
                end_ms: None,
                restarted: false,
                name: Some(name.into()),
                icon: Some("mark.png".into()),
                control,
                skill_name: None,
                skill_icon: None,
            }),
        )
    };
    let (f, r) = replay(&[
        me(),
        hp(1, 1000),
        hit(10, 7, 100),
        buff(1, 137_800_071, "Defense Break", None),
        buff(2, 100_000_301, "Stun", Some(Control::Stun)),
        hp(30, 0),
    ]);
    let v = RecapView::build(&f[0], r.book(), Me::default(), &|_| 0);
    let a = &v.attackers[0];
    assert_eq!(a.debuffs[0].sources[0].name, "Defense Break");
    assert_eq!(a.debuffs[0].sources[0].icon.as_deref(), Some("mark.png"));
    assert!(a.cc[0].sources.is_empty());
}

/// A buff that names the skill that applied it (core PR #23) gives the crowd
/// control its ability, though no hit of that skill was seen.
#[test]
fn a_buff_names_its_skill() {
    use recap::view::{Me, RecapView};
    let stun = ev(
        20,
        EventKind::BuffAdded(Buff {
            entity: 1,
            instance: 1,
            abnormal: 100_000_301,
            caster: 7,
            skill: Some(13_070_230),
            level: 1,
            length_ms: Some(3_000),
            end_ms: None,
            restarted: false,
            name: Some("Stun".into()),
            icon: Some("stun.png".into()),
            control: Some(Control::Stun),
            skill_name: Some("Shadowstrike".into()),
            skill_icon: Some("shadowstrike.png".into()),
        }),
    );
    let (f, r) = replay(&[me(), hp(1, 1000), hit(10, 7, 100), stun, hp(30, 0)]);
    let v = RecapView::build(&f[0], r.book(), Me::default(), &|_| 0);
    let cc = &v.attackers[0].cc[0];
    assert_eq!((cc.name.as_str(), cc.sources[0].name.as_str()), ("Stun", "Shadowstrike"));
    assert_eq!(cc.sources[0].icon.as_deref(), Some("shadowstrike.png"));
}
