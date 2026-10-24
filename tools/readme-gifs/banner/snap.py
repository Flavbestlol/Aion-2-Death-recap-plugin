"""The README banner and the social preview, from banner.html / social.html.

    python3 ../shot.py        # first: out/details.png, the real details card
    python3 snap.py           # docs/banner.jpg (2560 x 800), docs/social-preview.jpg (2560 x 1280)

back.jpg is original art made for this project (generated, not from the game).
"""
import subprocess
from pathlib import Path
from playwright.sync_api import sync_playwright

HERE = Path(__file__).resolve().parent
DOCS = HERE.parents[2] / "docs"
PAGES = [("banner.html", 400, "banner.jpg"), ("social.html", 640, "social-preview.jpg")]

with sync_playwright() as pw:
    browser = pw.chromium.launch()
    for page, height, out in PAGES:
        p = browser.new_page(viewport={"width": 1280, "height": height}, device_scale_factor=2)
        p.goto((HERE / page).as_uri())
        p.evaluate("document.getElementById('b').classList.toggle('v2', location.pathname.endsWith('social.html'))")
        p.wait_for_timeout(800)
        png = HERE / "tmp.png"
        p.locator("#b").screenshot(path=str(png))
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", str(png), "-q:v", "3", str(DOCS / out)], check=True)
        png.unlink()
        print(out)
    browser.close()
