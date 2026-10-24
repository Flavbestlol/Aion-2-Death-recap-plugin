"""Stills of the real overlay on a transparent background, for the banner.

    python3 shot.py   ->  out/bar.png, out/details.png  (2x)
"""
from pathlib import Path
from playwright.sync_api import sync_playwright
import rec

HERE = Path(__file__).resolve().parent


def main():
    rec.prepare()
    srv = rec.serve()
    with sync_playwright() as pw:
        b = pw.chromium.launch()
        ctx = b.new_context(viewport={"width": 1280, "height": 720}, device_scale_factor=2)
        ctx.add_init_script(path=str(HERE / "fake_host.js"))
        p = ctx.new_page()
        p.goto(f"http://127.0.0.1:{rec.PORT}/stage.html")
        p.add_style_tag(content="html,body{background:transparent!important}#bg{display:none}")
        p.wait_for_timeout(1500)
        r = {"fight_id": 1, "view": rec.VIEW, "extras": {},
             "history": {"Sampleton": [{"at_ms": rec.VIEW["at_ms"] - 1_500_000, "killer": "Sampleton", "skill": "Shadow Fall", "duel": False, "server": 1307},
                                       {"at_ms": rec.VIEW["at_ms"], "killer": "Sampleton", "skill": "Flash Slice", "duel": False, "server": 1307}]}}
        p.evaluate("document.getElementById('overlay').style.display='block'")
        p.evaluate("(r) => { window.top.__state.recap = r }", r)
        f = p.frame(name="overlay")
        f.evaluate("([e, x]) => window.__emit(e, x)", ["recap", r])
        for id_, gs, cp in [(rec.KILLER, 2718, 140596), (rec.OPENER, 2741, 151200), (rec.FOURTH, 2690, 132800), (rec.THIRD, 2655, 128400)]:
            f.evaluate("([e, x]) => window.__emit(e, x)", ["extra", {"fight_id": 1, "id": id_, "extra": rec.extras_for(id_, gs, cp)}])
        p.wait_for_timeout(1200)
        p.locator("#overlay").screenshot(path=str(rec.OUT / "bar.png"), omit_background=True)
        f.evaluate("([e, x]) => window.__emit(e, x)", ["alt", True])
        p.wait_for_timeout(1500)
        p.locator("#overlay").screenshot(path=str(rec.OUT / "details.png"), omit_background=True)
        b.close()
    srv.shutdown()


if __name__ == "__main__":
    main()
