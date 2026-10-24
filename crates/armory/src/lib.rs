//! A character's combat power and gear score, from the official AION 2 site.
//!
//! The game sends other players' combat power in their appear records (see
//! Flav-Toolkit's `player_profile` event), as it was at that moment, but never their gear score.
//! The official character site has both, by name and server: the same public
//! search and profile the site's own character page uses
//! (`aion2.plaync.com/en-us/characters`).
//!
//! This is the one part of Death Recap that uses the network, and only when
//! asked: it sends a character's name and server to the official site, nothing
//! else. It is polite to the site: one request at a time, at least a second
//! apart; answers kept for 10 minutes; and after the site says "too many" (403
//! or 429) it asks nothing for 10 minutes.
//!
//! The server list (`data/servers.json`, 106 servers in 5 regions) comes from
//! eiruuru/aion2-char-sheet (GPL-3.0), `src/api/servers.ts`, commit 0af5d9f.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const SEARCH: &str = "https://api-search.plaync.com/aion2global/search/v2/character";
const INFO: &str = "https://aion2.plaync.com/api/character/info";
const EQUIPMENT: &str = "https://aion2.plaync.com/api/character/equipment";
const SERVERS: &str = include_str!("../data/servers.json");

/// A game server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    #[serde(default)]
    pub id: u16,
    /// `eu`, `nae`, `naw`, `as` or `la`.
    pub region: String,
    pub name: String,
    /// As the game tags names: `LUM` in `Elsa[LUM]`.
    pub short: String,
    pub faction: String,
}

fn servers() -> &'static HashMap<u16, Server> {
    static ALL: OnceLock<HashMap<u16, Server>> = OnceLock::new();
    ALL.get_or_init(|| {
        let raw: HashMap<String, Server> = serde_json::from_str(SERVERS).expect("servers.json");
        raw.into_iter()
            .filter_map(|(id, mut s)| {
                let id = id.parse().ok()?;
                s.id = id;
                Some((id, s))
            })
            .collect()
    })
}

/// A server by its id (the game's, as `my_server` and `player_server` say it).
pub fn server(id: u16) -> Option<Server> {
    servers().get(&id).cloned()
}

/// A character's page on the official site. The site needs the region in
/// the address: without it the page redirects to its 404.
pub fn page_url(profile: &Profile) -> Option<String> {
    let region = server(profile.server_id)?.region;
    Some(format!(
        "https://aion2.plaync.com/en-us/characters/{}/{}?region={region}",
        profile.server_id,
        urlencoding::encode(&profile.character_id)
    ))
}

/// Servers by the short tag the game shows (`LUM`) or their name: one per
/// region and faction it exists in.
pub fn servers_named(tag: &str) -> Vec<Server> {
    let mut found: Vec<Server> = servers()
        .values()
        .filter(|s| s.short.eq_ignore_ascii_case(tag) || s.name.eq_ignore_ascii_case(tag))
        .cloned()
        .collect();
    found.sort_by_key(|s| s.id);
    found
}

/// What the official site says about a character.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub server_id: u16,
    pub server_name: String,
    pub class: String,
    pub level: i64,
    pub combat_power: i64,
    /// The site's item level ("gear score").
    pub gear_score: Option<i64>,
    /// `Elyos` or `Asmodian`.
    pub faction: String,
    /// The character's profile picture on the official site.
    pub portrait: Option<String>,
    /// The site's id for the character (for `equipment`).
    pub character_id: String,
}

/// One item a character wears, as the official site lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// `MainHand`, `Helmet`, `Ring1`…
    pub slot: String,
    pub name: String,
    /// `Common`, `Rare`, `Legend`, `Unique`, `Special`…
    pub grade: String,
    pub enchant: i64,
    pub exceed: i64,
    /// The game's icon for it.
    pub icon: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    UnknownServer(u16),
    /// No character of that exact name on that server.
    NotFound,
    /// Several characters of that exact name in the region: their servers.
    Ambiguous(Vec<u16>),
    /// The site said "too many" a short while ago: no request until then.
    Held {
        seconds_left: u64,
    },
    /// The site answered with this HTTP status.
    Status(u16),
    Network(String),
    /// The site answered something that is not the data it gives its own pages.
    Unexpected(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnknownServer(id) => write!(f, "no server {id} in the server list"),
            Error::NotFound => write!(f, "no character of that name on that server"),
            Error::Ambiguous(servers) => write!(f, "several characters of that name, on servers {servers:?}"),
            Error::Held { seconds_left } => write!(f, "the site asked to slow down: next try in {seconds_left} s"),
            Error::Status(s) => write!(f, "the site answered {s}"),
            Error::Network(e) => write!(f, "network: {e}"),
            Error::Unexpected(e) => write!(f, "unexpected answer: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// How the armory reaches the site; tests give it saved answers instead.
pub trait Transport: Send + Sync {
    /// GET `url`: the HTTP status and the body.
    fn get(&self, url: &str) -> Result<(u16, String), String>;
}

/// The real site, over HTTPS.
pub struct Http(ureq::Agent);

impl Default for Http {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(15)))
            .build();
        Http(ureq::Agent::new_with_config(config))
    }
}

impl Transport for Http {
    fn get(&self, url: &str) -> Result<(u16, String), String> {
        let mut response = self
            .0
            .get(url)
            .header("accept", "application/json")
            .header("referer", "https://aion2.plaync.com/en-us/characters/index")
            .header("origin", "https://aion2.plaync.com")
            .header(
                "user-agent",
                concat!(
                    "Death-Recap/",
                    env!("CARGO_PKG_VERSION"),
                    " (+https://github.com/Flavbestlol/Aion-2-Death-recap-plugin)"
                ),
            )
            .call()
            .map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
        Ok((status, body))
    }
}

/// How often the armory may ask, and how long it remembers.
#[derive(Debug, Clone, Copy)]
pub struct Pace {
    /// Between two requests.
    pub gap: Duration,
    /// How long an answer is kept.
    pub keep: Duration,
    /// How long to stop after the site says "too many".
    pub hold: Duration,
}

impl Default for Pace {
    fn default() -> Self {
        Pace { gap: Duration::from_secs(1), keep: Duration::from_secs(600), hold: Duration::from_secs(600) }
    }
}

#[derive(Default)]
struct State {
    last_request: Option<Instant>,
    held_until: Option<Instant>,
    cache: HashMap<(u16, String), (Instant, Result<Profile, Error>)>,
    gear: HashMap<String, (Instant, Result<Vec<Item>, Error>)>,
}

/// Looks characters up on the official site, politely.
pub struct Armory<T: Transport = Http> {
    transport: T,
    pace: Pace,
    state: Mutex<State>,
}

impl Default for Armory<Http> {
    fn default() -> Self {
        Self::new()
    }
}

impl Armory<Http> {
    pub fn new() -> Self {
        Self::with(Http::default(), Pace::default())
    }
}

impl<T: Transport> Armory<T> {
    pub fn with(transport: T, pace: Pace) -> Self {
        Armory { transport, pace, state: Mutex::new(State::default()) }
    }

    /// A character's combat power and gear score, by exact name and server
    /// id. Blocks for the pace between requests (two per lookup).
    pub fn lookup(&self, name: &str, server_id: u16) -> Result<Profile, Error> {
        let server = server(server_id).ok_or(Error::UnknownServer(server_id))?;
        let key = (server_id, name.to_string());
        if let Some((at, answer)) = self.lock().cache.get(&key)
            && at.elapsed() < self.pace.keep
        {
            return answer.clone();
        }
        let answer = self.fetch(name, &server);
        // "Too many" and network trouble are not answers about the character.
        if !matches!(answer, Err(Error::Held { .. } | Error::Network(_) | Error::Status(_))) {
            self.lock().cache.insert(key, (Instant::now(), answer.clone()));
        }
        answer
    }

    /// What a character wears (from `lookup`'s profile). One request, kept
    /// as long as profiles are.
    pub fn equipment(&self, profile: &Profile) -> Result<Vec<Item>, Error> {
        let key = profile.character_id.clone();
        if let Some((at, answer)) = self.lock().gear.get(&key)
            && at.elapsed() < self.pace.keep
        {
            return answer.clone();
        }
        let region = server(profile.server_id).map(|s| s.region).ok_or(Error::UnknownServer(profile.server_id))?;
        let answer = self
            .get(&format!(
                "{EQUIPMENT}?lang=en-US&region={region}&serverId={}&characterId={}",
                profile.server_id,
                urlencoding::encode(&profile.character_id)
            ))
            .map(|v| items(&v));
        if !matches!(answer, Err(Error::Held { .. } | Error::Network(_) | Error::Status(_))) {
            self.lock().gear.insert(key, (Instant::now(), answer.clone()));
        }
        answer
    }

    /// A character by exact name anywhere in a region (`eu`, `nae`, `naw`,
    /// `as`, `la`), for when their server is not known: found only when
    /// exactly one character in the region has that name.
    pub fn lookup_in_region(&self, name: &str, region: &str) -> Result<Profile, Error> {
        let search = self.get(&format!(
            "{SEARCH}?keyword={}&region={region}&localeInfo=en-US&page=1&size=50",
            urlencoding::encode(name),
        ))?;
        let hits: Vec<&Value> = search["list"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|hit| plain(hit["name"].as_str().unwrap_or_default()) == name)
            .collect();
        match hits.as_slice() {
            [] => Err(Error::NotFound),
            [one] => {
                let server_id = one["serverId"].as_u64().and_then(|s| u16::try_from(s).ok()).ok_or(Error::NotFound)?;
                self.lookup(name, server_id)
            }
            many => {
                Err(Error::Ambiguous(many.iter().filter_map(|h| h["serverId"].as_u64().map(|s| s as u16)).collect()))
            }
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn fetch(&self, name: &str, server: &Server) -> Result<Profile, Error> {
        let search = self.get(&format!(
            "{SEARCH}?keyword={}&region={}&localeInfo=en-US&serverId={}&page=1&size=10",
            urlencoding::encode(name),
            server.region,
            server.id
        ))?;
        let id = search["list"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|hit| {
                plain(hit["name"].as_str().unwrap_or_default()) == name
                    && hit["serverId"].as_u64() == Some(server.id.into())
            })
            .and_then(|hit| hit["characterId"].as_str())
            .ok_or(Error::NotFound)?;
        // The search gives the id URL-encoded (`…%3D`), the profile wants it plain.
        let id = urlencoding::decode(id).map(|s| s.into_owned()).unwrap_or_else(|_| id.to_string());
        let info = self.get(&format!(
            "{INFO}?lang=en-US&region={}&serverId={}&characterId={}",
            server.region,
            server.id,
            urlencoding::encode(&id)
        ))?;
        profile(&info)
    }

    /// One request, paced; "too many" holds every request after it.
    fn get(&self, url: &str) -> Result<Value, Error> {
        {
            let mut state = self.lock();
            if let Some(until) = state.held_until {
                let now = Instant::now();
                if now < until {
                    return Err(Error::Held { seconds_left: (until - now).as_secs().max(1) });
                }
                state.held_until = None;
            }
            if let Some(last) = state.last_request {
                let wait = self.pace.gap.saturating_sub(last.elapsed());
                if !wait.is_zero() {
                    std::thread::sleep(wait);
                }
            }
            state.last_request = Some(Instant::now());
        }
        let (status, body) = self.transport.get(url).map_err(Error::Network)?;
        match status {
            200 => serde_json::from_str(&body).map_err(|_| Error::Unexpected("not JSON".into())),
            404 => Err(Error::NotFound),
            403 | 429 => {
                self.lock().held_until = Some(Instant::now() + self.pace.hold);
                Err(Error::Held { seconds_left: self.pace.hold.as_secs() })
            }
            s => Err(Error::Status(s)),
        }
    }
}

/// A profile from the site's `character/info` answer.
fn profile(info: &Value) -> Result<Profile, Error> {
    let p = &info["profile"];
    let text = |k: &str| p[k].as_str().map(str::to_string).ok_or_else(|| Error::Unexpected(format!("no {k}")));
    let number = |k: &str| p[k].as_i64().ok_or_else(|| Error::Unexpected(format!("no {k}")));
    let gear_score = info["stat"]["statList"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|s| s["type"] == "ItemLevel")
        .and_then(|s| s["value"].as_i64());
    Ok(Profile {
        name: text("characterName")?,
        server_id: u16::try_from(number("serverId")?).map_err(|_| Error::Unexpected("serverId".into()))?,
        server_name: text("serverName")?,
        class: text("className")?,
        level: number("characterLevel")?,
        combat_power: number("combatPower")?,
        gear_score,
        faction: text("raceName").unwrap_or_default(),
        portrait: p["profileImage"].as_str().map(str::to_string),
        character_id: text("characterId")?,
    })
}

/// The worn items of the site's `character/equipment` answer.
fn items(v: &Value) -> Vec<Item> {
    v["equipment"]["equipmentList"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            Some(Item {
                slot: e["slotPosName"].as_str()?.to_string(),
                name: e["name"].as_str()?.to_string(),
                grade: e["grade"].as_str().unwrap_or_default().to_string(),
                enchant: e["enchantLevel"].as_i64().unwrap_or(0),
                exceed: e["exceedLevel"].as_i64().unwrap_or(0),
                icon: e["icon"].as_str()?.to_string(),
            })
        })
        .collect()
}

/// `<strong>Elsa</strong>` → `Elsa`: the search marks the matched part.
fn plain(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut in_tag = false;
    for c in name.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}
