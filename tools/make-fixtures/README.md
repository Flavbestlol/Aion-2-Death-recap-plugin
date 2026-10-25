# make-fixtures

Builds Death Recap's test fixtures from Flav-Toolkit's engine, and checks this repository's recap against the one that used to be built into Flav-Toolkit.

It needs Flav-Toolkit checked out beside this repository, at the commit `Cargo.toml` pins, with the meter's parser synced:

```text
<folder>/flav-toolkit/                     git checkout <pinned commit>; python tools/sync_upstream.py
<folder>/Aion-2-Death-recap-plugin/        this repository
```

```bash
cd tools/make-fixtures
cargo run --release -- fixtures             # frames/*.txt → crates/recap/tests/fixtures/*.events.jsonl, views checked against *.views.json
```

`frames/` holds the core's frame fixtures (ids only, `<Unix ms> <hex>` per line). `fixtures` feeds them to the engine exactly as the core's tests did, with the same `me` events at the same moments, writes what the feed would send, and prints every difference between this repository's views and `*.views.json`.

`*.views.json` is what the built-in recap showed for those frames. The built-in left the core in its PR #30, so these files are kept as they are, the reference, and never rewritten. They were made against the core at `074800a`, where this tool also compared a whole recorded session fight by fight (`compare`, in this tool's history at that pin; a session holds other players' names and chat, so it stays on your PC).
