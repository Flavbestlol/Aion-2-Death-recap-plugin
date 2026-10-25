//! Death Recap's fixtures from the core.
//!
//! ```text
//! cargo run --release -- fixtures          frames/*.txt → crates/recap/tests/fixtures/*.events.jsonl, checked against *.views.json
//! ```
//!
//! `fixtures` runs the pinned core's engine over the frame fixtures exactly as
//! the core's own tests did (the same `me` events at the same moments), writes
//! the events one JSON line each, as the feed sends them, and builds this
//! repository's views from them. It checks those views against
//! `*.views.json`: what the built-in Death Recap showed for the same frames.
//!
//! The built-in recap (`flav-recap`) left the core in its PR #30, so those
//! views are kept as they are, never rewritten here: they are the reference.
//! They were made, and a whole recorded session compared fight by fight (63
//! fights, no difference), against the core at `074800a`; this tool's history
//! at that pin still has the `compare` command that did it.

use std::path::PathBuf;

use flav_core::{Engine, Event as CoreEvent, EventKind as CoreKind};
use serde_json::Value;

/// The player's own combat power and gear score in the views: made-up numbers.
const ME_CP: i64 = 150_000;
const ME_GS: i32 = 2_600;

fn frames(text: &str) -> Vec<(i64, Vec<u8>)> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let (ms, hex) = l.split_once(' ')?;
            // A line of odd length or other than hex digits is skipped.
            if hex.len() % 2 != 0 || !hex.is_ascii() {
                return None;
            }
            let bytes = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
                .collect::<Option<Vec<u8>>>()?;
            Some((ms.parse().ok()?, bytes))
        })
        .collect()
}

/// One fixture: its file, the player's id, and when the core's test said who
/// the player is (`None`: before everything, at time 0).
struct Fixture {
    file: &'static str,
    me: i32,
    me_at: Option<i64>,
}

const FIXTURES: &[Fixture] = &[
    Fixture { file: "duels-2026-10-21", me: 4765, me_at: Some(1_792_611_340_000) },
    Fixture { file: "duel-lost-2026-10-21", me: 4765, me_at: None },
    Fixture { file: "abyss-death-live-2026-10-21", me: 12_445, me_at: None },
];

/// This repository's side: events as JSON, as the feed sends them.
#[derive(Default)]
struct Ours {
    recorder: recap::Recorder,
    views: Vec<Value>,
}

impl Ours {
    fn push(&mut self, json: &str) {
        let Ok(e) = serde_json::from_str::<flav_events::Event>(json) else {
            eprintln!("not an event: {json}");
            return;
        };
        for f in self.recorder.push(&e) {
            self.view(f);
        }
    }
    fn view(&mut self, mut f: recap::Fight) {
        self.recorder.fill_in(&mut f);
        let me = recap::view::Me { combat_power: Some(ME_CP), gear_score: Some(ME_GS) };
        let v = recap::view::RecapView::build(&f, self.recorder.book(), me, &|_| 1);
        self.views.push(serde_json::to_value(&v).unwrap_or_default());
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("fixtures") => fixtures(),
        _ => eprintln!("usage: make-fixtures fixtures"),
    }
}

fn fixtures() {
    let out = root().join("crates/recap/tests/fixtures");
    let _ = std::fs::create_dir_all(&out);
    for fx in FIXTURES {
        let text = std::fs::read_to_string(root().join("tools/make-fixtures/frames").join(format!("{}.txt", fx.file)))
            .unwrap_or_default();
        let mut engine = Engine::new();
        // As the core's tests did: the player has a name, so the parser reads
        // their HP records.
        engine.storage().append_nickname_authoritative(fx.me, "Player");
        let mut ours = Ours::default();
        let mut lines = Vec::new();
        let me = |at_ms| CoreEvent { at_ms, kind: CoreKind::Me { id: fx.me as i64, name: None } };
        let mut said_me = false;
        let emit = |e: &CoreEvent, ours: &mut Ours, lines: &mut Vec<String>| {
            let json = serde_json::to_string(e).unwrap_or_default();
            ours.push(&json);
            lines.push(json);
        };
        if fx.me_at.is_none() {
            emit(&me(0), &mut ours, &mut lines);
            said_me = true;
        }
        for (ms, frame) in frames(&text) {
            for e in engine.feed("Client:1:2", &frame, ms) {
                emit(&e, &mut ours, &mut lines);
            }
            if !said_me && fx.me_at.is_some_and(|at| ms >= at) {
                emit(&me(ms), &mut ours, &mut lines);
                said_me = true;
            }
        }
        for f in ours.recorder.finish() {
            ours.view(f);
        }
        let _ = std::fs::write(out.join(format!("{}.events.jsonl", fx.file)), lines.join("\n") + "\n");
        // The built-in's views, kept as they were made (see the top of this file).
        let built_in: Vec<Value> = std::fs::read_to_string(out.join(format!("{}.views.json", fx.file)))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        println!("{}: {} events, {} fights", fx.file, lines.len(), ours.views.len());
        report(&built_in, &ours.views);
    }
}

/// Every difference between the two sides' views, by path.
fn report(built_in: &[Value], ours: &[Value]) {
    let mut n = 0;
    for (i, (a, b)) in built_in.iter().zip(ours).enumerate() {
        let mut diffs = Vec::new();
        diff(&format!("fight {}", i + 1), a, b, &mut diffs);
        for d in &diffs {
            println!("  {d}");
        }
        n += diffs.len();
    }
    if built_in.len() != ours.len() {
        println!("  fights: {} built in, {} from the events", built_in.len(), ours.len());
        n += 1;
    }
    println!("  {n} difference(s)");
}

fn diff(path: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                diff(&format!("{path}.{k}"), v, y.get(k).unwrap_or(&Value::Null), out);
            }
            for (k, v) in y {
                if !x.contains_key(k) {
                    out.push(format!("{path}.{k}: (none) vs {v}"));
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                out.push(format!("{path}: {} items vs {}", x.len(), y.len()));
            }
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                diff(&format!("{path}[{i}]"), p, q, out);
            }
        }
        _ if a != b => out.push(format!("{path}: {a} vs {b}")),
        _ => {}
    }
}
