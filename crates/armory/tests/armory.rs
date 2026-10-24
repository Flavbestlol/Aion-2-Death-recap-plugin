//! The armory against saved answers from the official site (the project
//! author's own character, 2026-10-21), with no network.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use armory::{Armory, Error, Item, Pace, Profile, Transport, server, servers_named};

const SEARCH: &str = include_str!("fixtures/search-sample.json");
const INFO: &str = include_str!("fixtures/info-sample.json");
const EQUIPMENT: &str = include_str!("fixtures/equipment-sample.json");

/// Answers by URL prefix, and remembers every URL asked.
#[derive(Clone, Default)]
struct Saved {
    asked: Arc<Mutex<Vec<String>>>,
    status: Arc<Mutex<u16>>,
    /// Another search answer, when set.
    search: Arc<Mutex<Option<String>>>,
}

impl Transport for Saved {
    fn get(&self, url: &str) -> Result<(u16, String), String> {
        self.asked.lock().unwrap().push(url.to_string());
        let status = *self.status.lock().unwrap();
        if status != 200 {
            return Ok((status, String::new()));
        }
        if url.starts_with("https://api-search.plaync.com/aion2global/search/v2/character?") {
            Ok((200, self.search.lock().unwrap().clone().unwrap_or_else(|| SEARCH.to_string())))
        } else if url.starts_with("https://aion2.plaync.com/api/character/info?") {
            Ok((200, INFO.to_string()))
        } else if url.starts_with("https://aion2.plaync.com/api/character/equipment?") {
            Ok((200, EQUIPMENT.to_string()))
        } else {
            Ok((404, String::new()))
        }
    }
}

fn armory() -> (Armory<Saved>, Saved) {
    let saved = Saved::default();
    *saved.status.lock().unwrap() = 200;
    let pace = Pace { gap: Duration::ZERO, keep: Duration::from_secs(600), hold: Duration::from_secs(600) };
    (Armory::with(saved.clone(), pace), saved)
}

#[test]
fn a_lookup_gives_combat_power_and_gear_score() {
    let (armory, saved) = armory();
    let me = armory.lookup("Sampleton", 1307).unwrap();
    assert_eq!(
        me,
        Profile {
            name: "Sampleton".into(),
            server_id: 1307,
            server_name: "Fregion".into(),
            class: "Ranger".into(),
            level: 40,
            combat_power: 123_456,
            gear_score: Some(2_715),
            faction: "Elyos".into(),
            portrait: Some(
                "https://profileimg.plaync.com/game_profile_images/aion2global/images?gameServerKey=1307&charKey=100000000000000001"
                    .into()
            ),
            character_id: "c2FtcGxlLWNoYXJhY3Rlci1pZC1mb3ItdGVzdHM=".into(),
        }
    );
    let asked = saved.asked.lock().unwrap();
    assert_eq!(asked.len(), 2);
    assert!(
        asked[0].contains("keyword=Sampleton") && asked[0].contains("region=eu") && asked[0].contains("serverId=1307")
    );
    // The search's id (`…%3D`) is passed on encoded once, not twice.
    assert!(asked[1].ends_with("characterId=c2FtcGxlLWNoYXJhY3Rlci1pZC1mb3ItdGVzdHM%3D"), "{}", asked[1]);
}

/// The site's character page needs the region (checked 2026-10-22: without
/// `?region=` it redirects to plaync.com/error/404).
#[test]
fn a_character_page_has_its_region() {
    let (armory, _) = armory();
    let me = armory.lookup("Sampleton", 1307).unwrap();
    assert_eq!(
        armory::page_url(&me).as_deref(),
        Some("https://aion2.plaync.com/en-us/characters/1307/c2FtcGxlLWNoYXJhY3Rlci1pZC1mb3ItdGVzdHM%3D?region=eu")
    );
}

#[test]
fn an_answer_is_kept_and_not_asked_again() {
    let (armory, saved) = armory();
    armory.lookup("Sampleton", 1307).unwrap();
    armory.lookup("Sampleton", 1307).unwrap();
    assert_eq!(saved.asked.lock().unwrap().len(), 2);
}

#[test]
fn only_the_exact_name_on_that_server_counts() {
    let (armory, _) = armory();
    assert_eq!(armory.lookup("Flavbes", 1307), Err(Error::NotFound));
    assert_eq!(armory.lookup("Sampleton", 2304), Err(Error::NotFound), "the search answer is for server 1307");
}

#[test]
fn too_many_holds_every_request_for_a_while() {
    let (armory, saved) = armory();
    *saved.status.lock().unwrap() = 429;
    assert!(matches!(armory.lookup("Sampleton", 1307), Err(Error::Held { .. })));
    *saved.status.lock().unwrap() = 200;
    // The site is fine again, but the armory waits out its hold.
    assert!(matches!(armory.lookup("Sampleton", 1307), Err(Error::Held { .. })));
    assert_eq!(saved.asked.lock().unwrap().len(), 1);
}

#[test]
fn an_unknown_server_asks_nothing() {
    let (armory, saved) = armory();
    assert_eq!(armory.lookup("Sampleton", 9999), Err(Error::UnknownServer(9999)));
    assert!(saved.asked.lock().unwrap().is_empty());
}

#[test]
fn servers_by_id_and_by_the_tag_the_game_shows() {
    let kai = server(1304).unwrap();
    assert_eq!(
        (kai.name.as_str(), kai.short.as_str(), kai.region.as_str(), kai.faction.as_str()),
        ("Kaisinel", "KAI", "eu", "Elyos")
    );
    let lum = servers_named("LUM");
    assert!(lum.iter().any(|s| s.id == 2304 && s.region == "eu" && s.faction == "Asmodian"));
    assert!(lum.len() > 1, "Lumiel exists in several regions");
}

/// The real site, for a character of your choosing. Run by hand:
/// `FLAV_ARMORY_NAME=<name> FLAV_ARMORY_SERVER=<id> cargo test -p armory -- --ignored`.
#[test]
#[ignore = "uses the network"]
fn the_real_site() {
    let (Ok(name), Ok(server)) = (std::env::var("FLAV_ARMORY_NAME"), std::env::var("FLAV_ARMORY_SERVER")) else {
        eprintln!("set FLAV_ARMORY_NAME and FLAV_ARMORY_SERVER to run this");
        return;
    };
    let me = Armory::new().lookup(&name, server.parse().expect("a server id")).unwrap();
    assert_eq!(me.name, name);
    assert!(me.combat_power > 0 && !me.class.is_empty() && !me.server_name.is_empty());
}

#[test]
fn a_name_found_once_in_the_region_needs_no_server() {
    let (armory, saved) = armory();
    let me = armory.lookup_in_region("Sampleton", "eu").unwrap();
    assert_eq!((me.server_id, me.combat_power, me.gear_score), (1307, 123_456, Some(2_715)));
    let asked = saved.asked.lock().unwrap();
    assert!(asked[0].contains("region=eu") && !asked[0].contains("serverId"), "{}", asked[0]);
}

#[test]
fn a_name_found_on_several_servers_is_not_guessed() {
    let (armory, saved) = armory();
    let two = SEARCH.replacen("\"pagination\"", "\"x\"", 1).replace(
        "\"list\":[",
        "\"list\":[{\"characterId\":\"other\",\"name\":\"<strong>Sampleton</strong>\",\"serverId\":2304,\"serverName\":\"Lumiel\",\"region\":\"eu\"},",
    );
    *saved.search.lock().unwrap() = Some(two);
    assert_eq!(armory.lookup_in_region("Sampleton", "eu"), Err(Error::Ambiguous(vec![2304, 1307])));
}

/// What a character wears, for the gear snapshot: the game's icons, grades
/// and enchant levels, as the official site lists them.
#[test]
fn equipment_comes_with_the_games_icons() {
    let (armory, saved) = armory();
    let me = armory.lookup("Sampleton", 1307).unwrap();
    let gear = armory.equipment(&me).unwrap();
    assert_eq!(gear.len(), 3);
    assert_eq!(
        gear[0],
        Item {
            slot: "MainHand".into(),
            name: "Splendent Wise Dragon Lord Bow".into(),
            grade: "Unique".into(),
            enchant: 15,
            exceed: 5,
            icon: "https://assets.playnccdn.com/static-aion2-gamedata/resources/Icon_WP_BW_0100_T06.png".into(),
        }
    );
    assert!(saved.asked.lock().unwrap()[2].ends_with("characterId=c2FtcGxlLWNoYXJhY3Rlci1pZC1mb3ItdGVzdHM%3D"));
    // Kept: not asked again.
    armory.equipment(&me).unwrap();
    assert_eq!(saved.asked.lock().unwrap().len(), 3);
}
