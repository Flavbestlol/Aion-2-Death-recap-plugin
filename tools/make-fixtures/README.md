# make-fixtures

Builds Death Recap's test fixtures from Flav-Toolkit's engine, and checks a recorded session against the recap that used to be built into Flav-Toolkit.

It needs Flav-Toolkit checked out beside this repository, at the commit `Cargo.toml` pins, with the meter's parser synced:

```text
<folder>/flav-toolkit/                     git checkout <pinned commit>; python tools/sync_upstream.py
<folder>/Aion-2-Death-recap-plugin/        this repository
```

```bash
cd tools/make-fixtures
cargo run --release -- fixtures             # frames/*.txt → crates/recap/tests/fixtures/*.events.jsonl + *.views.json
cargo run --release -- compare <capture>    # a whole recorded session, fight by fight; prints differences, writes nothing
```

`frames/` holds the core's frame fixtures (ids only, `<Unix ms> <hex>` per line). `fixtures` feeds them to the engine exactly as the core's tests did, with the same `me` events at the same moments, and writes what the feed would send and what the built-in recap showed. A capture given to `compare` holds other players' names and chat: it stays on your PC, never in the repository.
