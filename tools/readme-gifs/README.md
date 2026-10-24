# readme-gifs

The GIFs and the banner on the README, made without recording the game: Death Recap's real pages (`app/ui`) run in headless Chromium over a clean game screenshot, a fake host (`fake_host.js`) stands in for Tauri, and a made-up fight (`view4.json`: Sampleton, Exampla, Quillon, Vesperine, with `items.json` for the gear card) is pushed to them. Playwright records the browser; ffmpeg cuts the GIFs.

It needs Python with `playwright` (Chromium installed) and `ffmpeg`.

```bash
python tools/class_icons.py                # once per clone: the class emblems
cd tools/readme-gifs
# put two clean screenshots of the game here: bg1.png (a field), bg2.png (a sky)
python3 rec.py                             # out/overview.gif, details.gif, lookups.gif, looks.gif
python3 rec.py details                     # one of them
python3 shot.py                            # out/bar.png, out/details.png: the overlay alone, 2x
python3 banner/snap.py                     # docs/banner.jpg, docs/social-preview.jpg, docs/how-it-works.png
cp out/*.gif ../../docs/                   # after looking at them
```

- **The screenshots never go in the repository** (`.gitignore` refuses them): no names, no chat, no party frames, and they are the game's pictures. Blur or replace any name first.
- `rec.py` builds `www/` itself: a fresh copy of `app/ui`, the stage, the two screenshots, and free look-alikes of the fonts Windows has (Barlow Semi Condensed for Bahnschrift, Open Sans for Segoe UI; Cinzel for the banner's title), fetched from Google Fonts' repository. Nothing in `www/`, `out/` or `tmp/` is committed.
- Each GIF is a storyboard in `rec.py` (`overview`, `details`, `lookups`, `looks`): captions, the drawn cursor, the events pushed and when. Re-run it whenever the pages change.
- `banner/back.jpg` is original art made for this project (generated, not from the game); `docs/emblem.png` too.
