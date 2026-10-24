# Death Recap

When you die in AION 2, Death Recap shows who killed you, with what, and what controlled you: a bar at the top of the screen, and each attacker's damage, skills and crowd control when you hold **Alt** (or click the bar). An add-on of [Flav-Toolkit](https://github.com/Flavbestlol/flav-toolkit): the core reads the game, Death Recap reads the core's local feed.

- **The bar**: the killer (portrait or class emblem, the final blow), their combat power and gear score against yours, everyone who hit you, how long you could not act, and what you dealt back.
- **The details** (Alt, or a click): per attacker, when they joined, damage and crits, their skills with the game's icons, which ability controlled you (skill, passive or Theostone), how many times and for how long, and what you dealt them.
- **Monsters and bosses** get a drawn mark and a *Monster* or *Boss* tag.
- **Who hunts you**: a red number on a player who killed you before; hover it for when and with what. Kept for this session, 24 hours or 3 days (your choice).
- **The skull by the minimap** (shown while the game is in front): a click opens the last recap any time; a right-click opens the settings; drag it where you like.
- **In the hub and on the dock**: a death raises a note with the killer's class emblem (the dock's skull lights up), taken back when you are up again. The hub's **Open** shows the last recap, **Settings** opens Death Recap's settings.
- **Online lookups** (off until you turn them on): an attacker's gear score, official portrait and gear, from the official AION 2 site. Hover a portrait for the gear; click a name for their character page.

## Install

Flav-Toolkit's hub lists it once its folder holds `death-recap.exe` and `addon.json` (`Addons\Death Recap\` during development); **Start** runs it, **Stop** ends it. It needs Flav-Toolkit running: without it, it waits.

## Privacy and rules

- **Read-only**: it reads Flav-Toolkit's local feed. It never touches the game, its files or its memory, and sends nothing to it.
- **Local**: no telemetry. Its settings and the history (when you keep it longer than a session) stay in `%APPDATA%\com.flavbest.death-recap\`.
- **Network**: the game's icons and font from the game's own servers, and, only with online lookups on, the official AION 2 site (an attacker's name and server, nothing else). Every host is listed in `addon.json`.
- GPL-3.0, like the parser Flav-Toolkit is built on.

## Build

```bash
python tools/class_icons.py              # once per clone: the class emblems (fetched, not committed)
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cargo build --release -p death-recap     # Windows: target/release/death-recap.exe
cargo run -p death-recap --example print # no windows: each recap as a line (beside `flav feed <capture>`)
```

The core is pinned by commit in `Cargo.toml` (`flav-events`, `flav-sdk`). How it works: [`docs/DESIGN.md`](docs/DESIGN.md).
