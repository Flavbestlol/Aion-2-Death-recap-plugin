# Working in this repository

Death Recap, an add-on of Flav-Toolkit (github.com/Flavbestlol/flav-toolkit): it reads the core's local feed and shows who killed you in AION 2. GPL-3.0, public, maintained by Flavbest with Claude sessions. This repository belongs to the session **Aion 2 - Death recap plugin**. `docs/DESIGN.md` is how it works; the add-on contract is the project doc `flav-toolkit/ADDONS.md`, and `flav-toolkit/SESSIONS.md` says who owns what (both are project docs, not files here).

## Before changing anything

- `git fetch` and branch from a fresh `main`: one branch per feature, a pull request, CI green, then merge. No direct pushes to `main`, no force-pushes.
- Never change the core from here. A new event, field or fix in the core is asked for in `ADDONS.md` section 10.
- Upgrading the core's pin (`rev` in `Cargo.toml`) is a pull request of its own: re-run `tools/make-fixtures` against that core and keep every test green.

## Commands

```bash
python tools/class_icons.py
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cargo build --release -p death-recap
```

CI runs exactly these on Windows and Linux and keeps `death-recap.exe` with `addon.json` as an artifact.

## Never

- Commit a capture or a recording. Fixtures hold ids only: `crates/recap/tests/fixtures` are events made by `tools/make-fixtures` from the core's frame fixtures, which hold ids only.
- Commit game art: class emblems are fetched by `tools/class_icons.py`, icons load from the game's servers at run time.
- Put anything personal in the code, docs, commits or releases: public name "Flavbest" only, no other players' names (test names are made up: "Sampleton"), no account, character or device ids, no session links.
- Send anything to the game, read its memory or files, or add telemetry. The only network use beyond the game's icons is the opt-in lookup on the official site.
- Install a build on Flavbest's PC without asking, or one not built from `main`.

## Commits and pull requests

The last lines of every commit message and pull request description are
`Co-Authored-By: Claude <noreply@anthropic.com>` and `Session: Aion 2 - Death recap plugin`.

## Installing on Flavbest's PC

- Only a build of `main` after the PR merged and CI passed; ask first.
- `death-recap.exe` and `addon.json` go to `F:\AI\Aion 2\Addons\Death Recap\`. Replaced files go to `F:\AI\Aion 2\Backups\Death Recap\<date> before <change>\`, recorded in `Backups\README.txt`; the install is recorded in `SESSIONS.md`.
