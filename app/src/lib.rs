//! Death Recap: when you die in AION 2, who killed you, with what, and what
//! controlled you, at the top of the screen.
//!
//! An add-on of Flav-Toolkit: it reads the core's local feed (never the game
//! itself), keeps who killed you, and, only when the player turns it on,
//! looks attackers up on the official AION 2 site. The windows are in
//! `main.rs`; everything they show comes from here.

pub mod reader;
pub mod settings;
pub mod store;

/// The `addon.json` id: never changes.
pub const ID: &str = "death-recap";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How Death Recap introduces itself to the feed.
pub fn config() -> flav_sdk::Config {
    flav_sdk::Config::new(ID, VERSION, reader::TYPES)
}
