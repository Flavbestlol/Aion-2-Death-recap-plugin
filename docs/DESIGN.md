# Death Recap: design

Status: 2026-10-27, v0.3.5. Moved out of Flav-Toolkit, where it was built in (decision 2 in the add-on contract: no add-on inside the core). Same recap, now its own program on the core's feed, pinned to the core at `58b7ffd` (0.18.0; its PR #4 lets `addon.json` name the add-on's emblem, its PR #30 adds the dock's `toggle`, its PR #64 shows add-ons' settings in the hub).

```text
AION 2 ──▶ Flav-Toolkit (reads the game) ──▶ the feed (ws://127.0.0.1, feed.json)
                                                │  events with names and icons on them
                                                ▼
             death-recap.exe:  reader ──▶ recap::Recorder ──▶ RecapView ──▶ the overlay, the note
                                  │                                            ▲
                                  └── (online lookups on) armory ── official site ┘
```

## Crates

| Crate | What it is |
|---|---|
| `crates/recap` | The recap's logic: `Recorder` cuts events into fights on the player; `view` turns a fight into what the overlay shows; `Book` keeps what the events said about names and icons. Depends on `flav-events` only (the core's event shapes) |
| `crates/armory` | The opt-in lookups on the official AION 2 site: gear score, portrait, worn items, the character page |
| `app` | `death-recap.exe` (Tauri): the reader (`reader.rs`, testable without windows), the online lookups (`lookups.rs`), settings and shared state (`settings.rs`, `store.rs`), the windows (`main.rs`), Alt (`alt.rs`), the pages (`ui/`) |
| `tools/make-fixtures` | Not in the workspace: builds the test fixtures from the core (needs a Flav-Toolkit checkout beside this one) |

The core is pinned by commit (`Cargo.toml`, `[workspace.dependencies]`): `flav-events` (the events), `flav-sdk` (the feed's client); `flav-feed` and `flav-addons` for tests.

## The reader (`app/src/reader.rs`)

- **Its emblem**: `emblem.png` beside `addon.json` (128 px, the winged skull of `docs/emblem.png`, a PNG of 40 KB; the core takes at most 64 KB), named by `"emblem"` in `addon.json`: the hub and the dock show it in their round icon. A test holds the manifest to the core's rules, the emblem included.
- **Connects** with `flav-sdk` as `death-recap`, asking for the types in `reader::TYPES` (the same list as `addon.json`, held together by a test). Flav-Toolkit not running: it waits and tries every half second; refused (an events version the core does not serve): it says so and tries every 5 s; the core restarted: it connects again, and the core re-sends who the player is at hello.
- **Every event** goes to `Recorder::push`; the player's own combat power, gear score and server are kept beside it, and the player's HP back above 0 after a death sends *revived* (the death's recap goes away).
- **Time**: fights also end by time (15 s of nothing, or 3 s after a death for the hits on their way). "Now" is the last event's time plus what has passed since it came, never the wall clock, so a replay (`flav feed`) of an old session ends its fights as the live game would.
- **A fight that ended** in a death or a lost duel (or any, with *every fight* on): who killed you goes into the history, `RecapView::build` makes the view, the screen shows it, and the hub and the dock get a note (below). The hub's log gets one line saying which rule opened it, with the HP that said so and no one's name (`reader::why`, 0.3.3: *Recap: died · HP 22,189 → 0 · 3 hits from 2 · final blow by a monster*), so a bug report shows why a recap appeared. With online lookups on, the fight's players go to the lookups' worker (below), and the overlay fills in as answers come.
- **The hub** sees Death Recap working while it calls `next` (a beat every 2 s), and its line says *N fights recorded*.
- **Notes** (`reader::death_note`): a death raises an `alert` note titled *Killed by Sampleton · 140.6K CP* (*Duel lost to …* for a duel; no power for a monster, or when the game did not say it), its line the class, the combat power against yours and the final blow (*Assassin · 140,596 combat power, 20% above yours · Flash Slice 5,677 crit*), the killer's class emblem as its picture (a monster: the final blow's icon from the game's data server). The title is what the dock's tip shows. The hub's Notes panel keeps it and the dock's skull lights up with it. It is taken back when the player is up again (their HP above 0) or the recap is closed; a new death replaces it. One note per fight.
- **The hub's buttons and the dock**: the core's `show` message reaches the reader as `Message::Show`: *Open* (the hub's button) shows the last recap (left up if it is), *Settings › Its own window* (and, before core 0.18.0, *Settings*) opens Death Recap's settings panel. Death Recap asks for the `toggle` feature in `hello` (0.3.2), so its icon on the dock sends *Toggle*: the last recap when none is shown, and a second click puts it away as its × does (the note comes down with it). Death Recap has no icon of its own: the core's dock is the way to it.

## Online lookups (`app/src/lookups.rs`, `crates/armory`)

Only while the player has them on: the worker asks the setting before every request, so turning lookups off drops what is left and sends nothing more. One worker thread, one fight at a time: a new fight's players replace what is left of the last one's.

- **Order**: `RecapView::by_importance`: who killed you, then who opened the fight (the earliest to join), then the rest by damage. Everyone's profile (portrait, combat power, gear score) is asked before anyone's gear, so the faces come first.
- **When the site refuses or fails**, it asks again rather than giving up: after "too many" (403/429) the armory stops every request for 20 s, twice as long each time the site says it again (at most 5 minutes), back to 20 s after an answer, and the worker waits that out and goes on in the same order; after a network or server error it tries again after 2, 4, 8… s (at most 30 s, 6 tries; "too many" does not use these up). An answer that will not change (no such character, several of that name) is not asked again. A fight's lookups stop after 10 minutes, waits included.
- **Each answer is sent as it comes** (`Screen::extra`): the profile first, then again with the gear (`gear_ready`), so the card can say the gear is on its way.
- **What each request carries** (`armory::HEADERS`): `Accept: application/json` and a `User-Agent` that names Death Recap, its version and this repository. Never an `Origin` or a `Referer`: Death Recap is not one of the site's own pages (since 0.3.1; the search, the profile and the gear answered identically without them for five public characters on 2026-10-25, as they did for Dragon Gear). A test holds it.
- **Portraits** come from the site's image server, which can refuse too: the page loads each aside, shows the class emblem until it is in, and asks again after 2, 4, 8, 15, 30, 60 and 120 s.
- **Gear**: each item's enchant level, and past +15 its diamond level (`exceedLevel` on the site; the site's blue diamond with the number).

## Names and icons come on the events (`crates/recap/src/book.rs`)

Flav-Toolkit puts the game data's names and icons on its events (PR #22 of the core): a hit or a miss carries its skill's name and icon, `npc_identified` the monster's name and whether it is a boss, a buff its name, icon and control. `name` events name entities. `Book` writes these down, bounded, and answers the view's questions; Death Recap keeps no tables of the game data.

- A specialised skill's id (`14110230`, on a buff) falls back to its base skill (`14110000`, the id hits carry), as the core's own lookup does.
- **Buffs name the skill that applied them** (`skill_name`, `skill_icon`, core PR #23): a passive that only marks, a stun that does no damage, a Theostone, each with its own name and icon, specialised ids included.
- **Checked**: over a 4-hour recorded session (63 fights, 13 deaths), the view built from the events alone equals the built-in recap's (which asked the core's engine directly) with no difference at all. Every attacker was named from `name` and `npc_identified` events alone.
- With a core older than PR #23 (no skill on buffs), a debuff from a skill no hit or miss named falls back: not crowd control, it is named after itself (Defense Break from Defense Break, as such marks are); crowd control keeps no source rather than a number.

## Heals that look like hits

The game sends an ally's heal on the player the way it sends a hit (a party Cleric's Radiant Benediction, 2026-10-21: 12 "hits" for 23,603 while the player stood at full HP). A Cleric's or Chanter's skill on the player (`17…`/`18…`, not the basic attack) waits up to 3 s for the next HP record of the player, and the HP decides:

- HP went up, or no record came (HP did not move: a heal at full HP): a heal, left out.
- HP went down: whichever reading is closer, all of them hits (down by them and the other hits since the last record) or all of them heals (down by the other hits, less up to them, as full HP can cut a heal short).
- A skill told apart twice the same way is known from then on; an entity that was hostile in the fight (a hit with a skill that cannot heal, a miss, crowd control) never heals.
- Every other hit counts at once. Hits decided late go into the fight at their own time.

An ally's buff renewed every few seconds kept fights open for minutes (one "fight" ran 316 s): only crowd control applied again keeps a fight going.

## Fights (`crates/recap/src/lib.rs`)

A fight starts with the first hit, dodge, resist or crowd control another entity lands on the player (another debuff joins a fight going on, or one opening within 2 s, but does not start one), and ends at a death (HP 0), a lost duel (HP stops at 1 right after **another player's** hit: AION 2 duels end at 1 HP; since 0.3.3 a monster's hit that leaves the player at 1 HP, as an effect that keeps them alive does, ends nothing and the fight goes on), or after 15 s with nothing. Hits within 3 s after a death or a duel's end stay with it as `after_end`. Each fight lists every attacker (damage per skill, crits, debuffs, how long their crowd control held the player, overlaps counted once, cut at the death), dodges and resists, the lowest HP, the final blow, and what the player dealt (from 10 s before the first hit). Events from before the game says who the player is are kept (5 minutes) and replayed once it does.

- **Crowd control** is the buff's `control` (the core reads it from the game data's name: stun, knock, fear, sleep, petrify, stagger, confusion are *hard*; root, slow, silence, blind, taunt are *soft*).
- A buff instance can move on to another effect (`buff_changed` with a new id): Shadow Fall turns a stun into its next step, then knocks down. The first effect ends there and the next starts.
- **Crowd control, by ability**: one entry per effect and the ability that put it (a skill, a passive that triggered, a Theostone), so two knockbacks from two skills are two.

## The windows (`app/src/main.rs`, `app/ui/`)

Same behaviour as when built in. **Two looks** (`settings.theme`): the HUD every Flav-Toolkit add-on shares (the default: warm white and gold on dark glass, rounded, round portraits, Bahnschrift for names and numbers, as the hub and Dragon Gear's overlay), and the game's own (charcoal, square corners, teal, the game's font; the look of 0.2.0). The pages hold both: `body[data-theme]`, the HUD rules at the end of each stylesheet.

- **The overlay** (`index.html`, `app.js`, `style.css`): top centre of the main screen, no decorations, transparent, no shadow, never takes the focus (`focusable(false)`, `WS_EX_NOACTIVATE`), sized to its content (a transparent window still catches clicks, so it is never bigger than what it shows; the height is worked out from what is shown, never measured while the details slide). It shows on a death or a lost duel and leaves when the player is back on their feet (a lost duel or a survived fight leaves by itself). Above the name it says why it opened (0.3.3): *Killed by*, *Duel lost to*, or *Fight over · top damage* (a survived fight, shown only with *every fight* on), so a recap never looks like it came from nowhere.
- **Details**: hold **Alt**, or click the bar (or its Alt key) to keep them open. Alt is read with `GetAsyncKeyState`, only while a recap shows; the game still gets the key. Windows hides the keyboard from a program while one running as administrator has the focus: when the game runs as administrator and Death Recap does not, the key on the bar says **Click**. Started by an elevated Flav-Toolkit, Death Recap is elevated too and reads Alt.
- **No icon of its own** (0.3.0): the core's dock has Death Recap's skull; its click is the `show` *Toggle* (0.3.2): open, then put away.
- **Settings** (`settings.*`, a small panel in the middle of the main screen, opened by the hub's *Settings* or by starting Death Recap again): online lookups (off by default), every fight, how long to remember who killed you (this session, a day, 3 days), the look (HUD or Game). × or Esc closes it and gives the game its focus back; clicking another window closes it and leaves that window in front. Kept in `%APPDATA%\com.flavbest.death-recap\settings.json`; a damaged file is set aside as `settings.json.damaged` and the defaults are used; on the first start (no file), the choices made in Flav-Toolkit when Death Recap was built in are read once from its folder (and its kill history, when kept longer than a session).
- **The same settings in Flav-Toolkit's hub** (0.3.5, core 0.18.0): Death Recap asks for the feed's `settings` feature and describes the four settings (`Settings::describe`: *Lookups* with online lookups, every fight and the history; *Look* with the look, linked to the shared `look`, so *Settings › All add-ons › Look* sets it with every other add-on's) on connecting and again after every change. A change made in the hub comes as `setting` and goes the panel's way (`Shared::change`: set, saved, the history kept or deleted as it now says); the overlay takes the look and an open panel shows the new values. A key or value Death Recap doesn't take changes nothing, and the hub is told the values it keeps. An older core ignores the feature: the panel is the only place, as before.
- **Character pages**: `https://aion2.plaync.com/en-us/characters/<server>/<id>?region=<region>` (`armory::page_url`); the page can open only pages the lookups found.
- **Icons**: the game's own, from its data server, as the events name them; class emblems from the meter's repository, fetched before a build (`tools/class_icons.py`) and compiled in; combat power and gear score marks and the font from the official site. Monsters get a drawn mark. **Portraits** are the official site's `profileImage` (`profileimg.plaync.com`), which answers with a redirect to the picture on `fizz-download.playnccdn.com`: the window's `img-src` allows both, or every portrait falls back to the class emblem (0.3.0 to 0.3.3, after the audit took the second host out; a test now holds both, and every host the window loads from is declared in `addon.json`).
- **One instance** (`tauri-plugin-single-instance`).

## Tests and fixtures

- `crates/recap/tests/fights.rs`: three real sessions (two duels, a lost duel, a live Abyss death) as the events the core's engine read from the core's frame fixtures (ids only), one JSON line each as the feed sends them; their views must equal the built-in recap's (`*.views.json`) field for field. Plus the fight rules on made-up events.
- `app/tests/against_the_feed.rs`: a real feed in the test process serves the Abyss death; the reader shows the recap, writes *1 fight recorded* in the hub and raises its note. The manifest is checked as the hub reads it.
- `app/tests/lookups.rs`: the lookups against a made-up site with no network: the order, a refusal and a failure asked again, no such character asked once, a site that keeps failing given up on, a new fight taking over.
- `tools/make-fixtures` (needs the core checked out beside this repository at the pinned commit): `fixtures` rebuilds both kinds of fixture from the frames in `tools/make-fixtures/frames/`; `compare <capture.txt>` checks a whole recorded session, fight by fight, against the built-in recap, and writes nothing (a session holds other players' names).
