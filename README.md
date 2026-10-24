# Death Recap

When you die in AION 2, Death Recap shows who killed you, with what, and what controlled you: a bar at the top of the screen, and each attacker's damage, skills and crowd control when you hold **Alt** (or click the bar). An add-on of [Flav-Toolkit](https://github.com/Flavbestlol/flav-toolkit): the core reads the game, Death Recap reads the core's local feed.

- **The bar**: the killer (portrait or class emblem, the final blow), their combat power and gear score against yours, everyone who hit you, how long you could not act, and what you dealt back.
- **The details** (Alt, or a click): per attacker, when they joined, damage and crits, their skills with the game's icons, which ability controlled you (skill, passive or Theostone), how many times and for how long, and what you dealt them.
- **Monsters and bosses** get a drawn mark and a *Monster* or *Boss* tag.
- **Who hunts you**: a red number on a player who killed you before (how many times, this death included); hover it for when and with what. Kept for this session, 24 hours or 3 days (your choice).
- **On Flav-Toolkit's dock and in its hub**: a death raises a note, *Killed by Sampleton · 140.6K CP*, with the killer's class emblem (the dock's skull lights up), taken back when you are up again. The dock's skull and the hub's **Open** show the last recap; the hub's **Settings** opens Death Recap's settings.
- **Online lookups** (off until you turn them on): an attacker's gear score, official portrait and gear, from the official AION 2 site, the killer first, then whoever opened the fight, then the rest; when the site is busy, they come a little later instead of not at all. Hover a portrait for the gear (enchant level, and the diamond level past +15); click a name for their character page.
- **Two looks**: the HUD every Flav-Toolkit add-on shares (the default), or the game's own; the settings switch them.

## Install

Flav-Toolkit's hub lists it once its folder holds `death-recap.exe` and `addon.json` (`Addons\Death Recap\` during development); **Start** runs it, **Stop** ends it. It needs Flav-Toolkit running: without it, it waits.

## Privacy and rules

- **Read-only**: it reads Flav-Toolkit's local feed. It never reads the game's memory or files, and sends nothing to the game. Two things it asks Windows about the game's window: whether the game runs as administrator (then Alt cannot be read, and the bar says to click), and, when you close Death Recap's settings with × or Esc, to give the game its focus back.
- **Local**: no telemetry. Its settings and the history (only when you keep it for a day or 3 days) stay in `%APPDATA%\com.flavbest.death-recap\`. Windows' web view keeps its usual cache (the icons and portraits it showed) in `%LOCALAPPDATA%\com.flavbest.death-recap\`.
- **Network**: the game's icons and font from the game's own servers, and, only while online lookups are on, the official AION 2 site (an attacker's name and server, then the site's own id for them; nothing about you). Turning lookups off stops them at once. Every host is listed in `addon.json`.
- GPL-3.0, like the parser Flav-Toolkit is built on.

## Build

```bash
python tools/class_icons.py              # once per clone: the class emblems (fetched, not committed)
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cargo build --release -p death-recap     # Windows: target/release/death-recap.exe
cargo run -p death-recap --example print # no windows: each recap as a line (beside `flav feed <capture>`)
```

The core is pinned by commit in `Cargo.toml` (`flav-events`, `flav-sdk`). How it works: [`docs/DESIGN.md`](docs/DESIGN.md).
