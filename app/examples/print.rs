//! Death Recap without its windows: each recap as one line, for checking it
//! against a replay (`flav feed <capture.txt>`) or the live game. Ctrl+C ends it.
//!
//! ```text
//! cargo run -p death-recap --example print
//! ```

use std::sync::Arc;

use death_recap::reader::{self, Screen};
use death_recap::store::{Extra, Recap, Shared};

struct Print;

impl Screen for Print {
    fn recap(&self, r: &Recap) {
        let v = &r.view;
        let by: Vec<String> = v.attackers.iter().map(|a| format!("{} {}", a.name, a.damage)).collect();
        let blow = v.final_blow.as_ref().map(|b| format!("{} {}", b.skill.name, b.damage)).unwrap_or_default();
        println!(
            "{:?}: {} damage in {} ms, by {}; final blow {blow}",
            v.outcome,
            v.damage,
            v.duration_ms,
            by.join(", ")
        );
    }
    fn extra(&self, _: i64, id: i32, extra: &Extra) {
        println!("  #{id}: gear score {:?}", extra.gear_score);
    }
    fn revived(&self) {
        println!("  back on your feet");
    }
    fn status(&self, text: &str) {
        println!("[{text}]");
    }
}

fn main() {
    let dir = std::env::temp_dir().join("death-recap-print");
    let shared = Shared::new(dir, None);
    reader::run(shared, Arc::new(Print), &death_recap::config(), &|| false);
}
