//! The online lookups, only when the player turned them on: each player who
//! attacked, on the official AION 2 site, the ones that matter first.
//!
//! One worker, one fight at a time: a new fight's attackers replace what is
//! left of the last one's. It asks for everyone's profile (portrait, combat
//! power, gear score) before anyone's gear, in the order of
//! `RecapView::by_importance`: who killed you, who opened the fight, then the
//! rest. When the site says "too many" or does not answer, it waits and asks
//! again (the armory sets how long), so a portrait comes late rather than
//! never; it stops on an answer that will not change ("no such character")
//! or when the fight's lookups have tried for 10 minutes. It sends the
//! character's name and server and nothing else (see `armory`).

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

use armory::{Armory, Error, Profile, Transport};

use crate::store::Extra;

/// One player to look up.
#[derive(Debug, Clone)]
pub struct Player {
    /// Their entity id in the fight.
    pub id: i32,
    pub name: String,
    pub server: Option<u16>,
}

/// A fight's attackers to look up, most important first.
#[derive(Debug, Clone)]
pub struct Request {
    pub fight_id: i64,
    pub players: Vec<Player>,
    /// The player's own region, for attackers whose server is not known.
    pub region: Option<String>,
}

/// What the site said about one attacker so far (sent again when the gear
/// comes in).
#[derive(Debug, Clone)]
pub struct Found {
    pub fight_id: i64,
    pub id: i32,
    pub extra: Extra,
    /// Their official character page.
    pub page: Option<String>,
}

/// How long the worker keeps trying.
#[derive(Debug, Clone, Copy)]
pub struct Patience {
    /// The wait after the site did not answer (a network error, a server
    /// error); twice as long each time after.
    pub first_wait: Duration,
    pub max_wait: Duration,
    /// Tries per request when the site does not answer ("too many" is waited
    /// out instead, as the armory says).
    pub tries: u32,
    /// A fight's lookups stop after this long.
    pub give_up_after: Duration,
}

impl Default for Patience {
    fn default() -> Self {
        Patience {
            first_wait: Duration::from_secs(2),
            max_wait: Duration::from_secs(30),
            tries: 6,
            give_up_after: Duration::from_secs(10 * 60),
        }
    }
}

enum Job {
    Profile(Player),
    Gear { id: i32, profile: Box<Profile> },
}

struct Try {
    job: Job,
    tries: u32,
}

/// One fight's lookups in progress.
struct Fight {
    id: i64,
    region: Option<String>,
    until: Instant,
    jobs: VecDeque<Try>,
    extras: HashMap<i32, Extra>,
}

impl Fight {
    fn new(r: Request, patience: &Patience) -> Fight {
        Fight {
            id: r.fight_id,
            region: r.region,
            until: Instant::now() + patience.give_up_after,
            jobs: r.players.into_iter().map(|p| Try { job: Job::Profile(p), tries: 0 }).collect(),
            extras: HashMap::new(),
        }
    }
}

/// What to do after one request.
enum Next {
    Done,
    /// Ask again after this long.
    Again(Duration),
}

/// Works until the sender is gone and nothing is left to do.
pub fn run<T: Transport>(armory: &Armory<T>, requests: Receiver<Request>, patience: Patience, found: &dyn Fn(Found)) {
    let mut fight: Option<Fight> = None;
    let mut open = true;
    loop {
        let idle = fight.as_ref().is_none_or(|f| f.jobs.is_empty());
        if idle {
            if !open {
                return;
            }
            match requests.recv() {
                Ok(r) => fight = Some(Fight::new(r, &patience)),
                Err(_) => return,
            }
        } else if open {
            match requests.try_recv() {
                Ok(r) => fight = Some(Fight::new(r, &patience)),
                Err(TryRecvError::Disconnected) => open = false,
                Err(TryRecvError::Empty) => {}
            }
        }
        let Some(f) = fight.as_mut() else { continue };
        if Instant::now() >= f.until {
            f.jobs.clear();
            continue;
        }
        let Some(t) = f.jobs.pop_front() else { continue };
        if let Next::Again(wait) = step(armory, f, t, &patience, found) {
            // A new fight ends the wait (and replaces this one).
            if open {
                match requests.recv_timeout(wait) {
                    Ok(r) => fight = Some(Fight::new(r, &patience)),
                    Err(RecvTimeoutError::Disconnected) => open = false,
                    Err(RecvTimeoutError::Timeout) => {}
                }
            } else {
                std::thread::sleep(wait);
            }
        }
    }
}

/// One request to the site for the fight's next job.
fn step<T: Transport>(armory: &Armory<T>, f: &mut Fight, t: Try, patience: &Patience, found: &dyn Fn(Found)) -> Next {
    match t.job {
        Job::Profile(p) => {
            let answer = match (p.server, &f.region) {
                (Some(s), _) => armory.lookup(&p.name, s),
                (None, Some(r)) => armory.lookup_in_region(&p.name, r),
                (None, None) => return Next::Done,
            };
            match answer {
                Ok(profile) => {
                    let page = armory::page_url(&profile);
                    let extra = Extra {
                        gear_score: profile.gear_score,
                        combat_power: Some(profile.combat_power),
                        portrait: profile.portrait.clone(),
                        items: Vec::new(),
                        page: page.is_some(),
                        gear_ready: false,
                    };
                    f.extras.insert(p.id, extra.clone());
                    found(Found { fight_id: f.id, id: p.id, extra, page });
                    // Gear after everyone's profile.
                    f.jobs.push_back(Try { job: Job::Gear { id: p.id, profile: Box::new(profile) }, tries: 0 });
                    Next::Done
                }
                Err(e) => again(f, Try { job: Job::Profile(p), tries: t.tries }, &e, patience),
            }
        }
        Job::Gear { id, profile } => {
            let items = match armory.equipment(&profile) {
                Ok(items) => items,
                Err(e) => match wait_for(&e, t.tries, patience) {
                    Some(wait) => {
                        f.jobs.push_front(Try { job: Job::Gear { id, profile }, tries: t.tries + 1 });
                        return Next::Again(wait);
                    }
                    // Nothing more will come: the card stops waiting for it.
                    None => Vec::new(),
                },
            };
            let mut extra = f.extras.get(&id).cloned().unwrap_or_default();
            extra.items = items;
            extra.gear_ready = true;
            f.extras.insert(id, extra.clone());
            found(Found { fight_id: f.id, id, extra, page: armory::page_url(&profile) });
            Next::Done
        }
    }
}

/// Puts the try back at the front (it keeps its place) when the error is
/// one that passes.
fn again(f: &mut Fight, t: Try, e: &Error, patience: &Patience) -> Next {
    match wait_for(e, t.tries, patience) {
        Some(wait) => {
            f.jobs.push_front(Try { job: t.job, tries: t.tries + 1 });
            Next::Again(wait)
        }
        None => Next::Done,
    }
}

/// How long to wait before asking again, or `None` when asking again would
/// get the same answer.
fn wait_for(e: &Error, tries: u32, patience: &Patience) -> Option<Duration> {
    match e {
        // The armory knows how long the site wants: always wait it out.
        Error::Held { seconds_left } => Some(Duration::from_secs(*seconds_left)),
        Error::Network(_) | Error::Status(500..=599) if tries + 1 < patience.tries => {
            Some(patience.first_wait.saturating_mul(1u32 << tries.min(16)).min(patience.max_wait))
        }
        _ => None,
    }
}
