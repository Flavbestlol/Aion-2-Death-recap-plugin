"""Death Recap's README GIFs: the real pages over a clean game screenshot,
fed made-up fights by a fake host, recorded headless and cut with ffmpeg.

    python3 rec.py [overview|details|lookups|looks]   (all when none given)

Needs two clean screenshots of the game (no names, no chat) as bg1.png and
bg2.png in this folder; they are never committed. Builds ./www from
app/ui and free look-alike fonts, serves it on 8766 itself, writes
out/<name>.gif; raw videos stay in tmp/. See README.md.
"""

import json, os, shutil, subprocess, sys, threading, time, functools, http.server, urllib.request
from pathlib import Path
from playwright.sync_api import sync_playwright

HERE = Path(__file__).resolve().parent
WWW, OUT, TMP = HERE / "www", HERE / "out", HERE / "tmp"
W, H = 1280, 720
PORT = 8766

VIEW = json.loads((HERE / "view4.json").read_text())
ITEMS = json.loads((HERE / "items.json").read_text())
KILLER, OPENER, THIRD, FOURTH = 12645, 10159, 12907, 10322

CHROME_JS = r"""
(() => {
  if (window !== window.top) return;
  addEventListener('DOMContentLoaded', () => {
    const css = document.createElement('style');
    css.textContent = `
      #__cur{position:fixed;left:0;top:0;width:26px;height:26px;z-index:99999;pointer-events:none;transform-origin:3px 3px;transition:transform .08s}
      #__cur.down{transform:scale(.85)}
      .__ring{position:fixed;width:34px;height:34px;margin:-17px 0 0 -17px;border-radius:50%;border:2px solid #f2c35b;z-index:99998;pointer-events:none;animation:__ping .55s ease-out forwards}
      @keyframes __ping{from{transform:scale(.3);opacity:1}to{transform:scale(1.4);opacity:0}}
      #__cap{position:fixed;z-index:99997;pointer-events:none;transform:translateX(-50%);padding:10px 20px 11px;border-radius:14px;
        background:rgba(8,10,15,.86);box-shadow:inset 0 0 0 1px rgba(242,195,91,.45),0 8px 24px rgba(0,0,0,.5);text-align:center;
        font-family:"Barlow Semi Condensed",sans-serif;color:#ece4d0;opacity:0;transition:opacity .25s}
      #__cap.on{opacity:1}
      #__cap b{display:block;font:600 22px/1.2 "Cap",sans-serif;color:#ffe7a6}
      #__cap span{display:block;font:500 16px/1.3 "Cap",sans-serif;color:#cfc8b4;margin-top:2px}
      #__key{position:fixed;z-index:99997;pointer-events:none;padding:6px 14px;border-radius:9px;background:rgba(8,10,15,.86);
        box-shadow:inset 0 0 0 1px rgba(242,195,91,.7);font:600 19px "Cap",sans-serif;color:#ffe7a6;opacity:0;transition:opacity .15s}
      #__key.on{opacity:1}
      @font-face{font-family:"Cap";font-weight:500;src:url(/fonts/BarlowSemiCondensed-Medium.ttf)}
      @font-face{font-family:"Cap";font-weight:600;src:url(/fonts/BarlowSemiCondensed-SemiBold.ttf)}`;
    document.head.append(css);
    const cur = document.createElement('div'); cur.id = '__cur';
    cur.innerHTML = '<svg viewBox="0 0 26 26" width="26" height="26"><path d="M3 2 L3 21 L8 16.5 L11.5 24 L14.5 22.6 L11 15.2 L17.5 15.2 Z" fill="#fff" stroke="#111" stroke-width="1.6" stroke-linejoin="round"/></svg>';
    document.body.append(cur);
    const cap = document.createElement('div'); cap.id = '__cap'; document.body.append(cap);
    const key = document.createElement('div'); key.id = '__key'; document.body.append(key);
    window.__curAt = (x, y) => { cur.style.transform = cur.classList.contains('down') ? `translate(${x}px,${y}px) scale(.85)` : `translate(${x}px,${y}px)`; cur.dataset.x = x; cur.dataset.y = y; };
    window.__down = (on) => { cur.classList.toggle('down', on); window.__curAt(+cur.dataset.x || 0, +cur.dataset.y || 0);
      if (on) { const r = document.createElement('div'); r.className = '__ring'; r.style.left = cur.dataset.x + 'px'; r.style.top = cur.dataset.y + 'px'; document.body.append(r); setTimeout(() => r.remove(), 700); } };
    window.__cap = (t, s) => { cap.replaceChildren(); if (t) { const b = document.createElement('b'); b.textContent = t; cap.append(b); }
      if (s) { const p = document.createElement('span'); p.textContent = s; cap.append(p); } cap.classList.toggle('on', !!(t || s)); };
    window.__capbox = (cx, maxw, bottom) => { cap.style.left = cx + 'px'; cap.style.maxWidth = maxw + 'px'; cap.style.bottom = bottom + 'px'; cap.style.top = 'auto'; };
    window.__key = (text, x, y) => { key.textContent = text || ''; key.classList.toggle('on', !!text); if (x != null) { key.style.left = x + 'px'; key.style.top = y + 'px'; } };
    window.__curAt(-50, -50);
  });
})();
"""


REPO = HERE.parents[1]
# Free look-alikes of the fonts Windows has (Bahnschrift, Segoe UI), SIL OFL.
FONTS = {
    "BarlowSemiCondensed-Medium.ttf": "ofl/barlowsemicondensed/BarlowSemiCondensed-Medium.ttf",
    "BarlowSemiCondensed-SemiBold.ttf": "ofl/barlowsemicondensed/BarlowSemiCondensed-SemiBold.ttf",
    "BarlowSemiCondensed-Bold.ttf": "ofl/barlowsemicondensed/BarlowSemiCondensed-Bold.ttf",
    "OpenSans.ttf": "ofl/opensans/OpenSans%5Bwdth,wght%5D.ttf",
    "Cinzel.ttf": "ofl/cinzel/Cinzel%5Bwght%5D.ttf",  # the banner's title
}


def prepare():
    """www/ = the stage, a fresh copy of the add-on's pages, the fonts and
    the two backgrounds. Nothing in it is committed."""
    for bg in ("bg1.png", "bg2.png"):
        if not (HERE / bg).exists():
            sys.exit(f"{bg} is missing: put a clean screenshot of the game here (see README.md)")
    if not (REPO / "app/ui/img/class").exists():
        sys.exit("the class emblems are missing: run python tools/class_icons.py first")
    shutil.rmtree(WWW / "ui", ignore_errors=True)
    shutil.copytree(REPO / "app/ui", WWW / "ui")
    for f in ("stage.html", "bg1.png", "bg2.png"):
        shutil.copy(HERE / f, WWW / f)
    (WWW / "fonts").mkdir(exist_ok=True)
    for name, path in FONTS.items():
        if not (WWW / "fonts" / name).exists():
            urllib.request.urlretrieve(f"https://raw.githubusercontent.com/google/fonts/main/{path}", WWW / "fonts" / name)


def serve():
    class Quiet(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *a):
            pass
    handler = functools.partial(Quiet, directory=str(WWW))
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", PORT), handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv


class Rec:
    def __init__(self, pw, name, bg="bg1.png"):
        self.name = name
        shutil.rmtree(TMP / name, ignore_errors=True)
        self.browser = pw.chromium.launch()
        self.ctx = self.browser.new_context(viewport={"width": W, "height": H}, record_video_dir=str(TMP / name),
                                            record_video_size={"width": W, "height": H})
        self.ctx.add_init_script(path=str(HERE / "fake_host.js"))
        self.ctx.add_init_script(CHROME_JS)
        self.page = self.ctx.new_page()
        self.t0 = time.monotonic()
        self.page.goto(f"http://127.0.0.1:{PORT}/stage.html")
        self.page.evaluate(f"document.getElementById('bg').style.backgroundImage='url({bg})'")
        self.page.wait_for_timeout(1500)
        self.x, self.y = W / 2, H * 0.75
        self.page.evaluate(f"__curAt({self.x},{self.y})")
        self.start = None

    # ---- the pages
    def frame(self, name):
        return self.page.frame(name=name)

    def emit(self, frame, event, payload=None):
        self.frame(frame).evaluate("([e, p]) => window.__emit(e, p)", [event, payload])

    def recap(self, extras=None):
        r = {"fight_id": 1, "view": VIEW, "extras": extras or {},
             "history": {"Sampleton": [{"at_ms": VIEW["at_ms"] - 1_500_000, "killer": "Sampleton", "skill": "Shadow Fall", "duel": False, "server": 1307},
                                       {"at_ms": VIEW["at_ms"], "killer": "Sampleton", "skill": "Flash Slice", "duel": False, "server": 1307}]}}
        self.page.evaluate("document.getElementById('overlay').style.display='block'")
        self.page.evaluate("(r) => { window.top.__state.recap = r }", r)
        self.emit("overlay", "recap", r)

    def extra(self, id_, extra):
        self.emit("overlay", "extra", {"fight_id": 1, "id": id_, "extra": extra})

    # ---- timing and captions
    def mark(self):
        self.start = time.monotonic() - self.t0

    def wait(self, ms):
        self.page.wait_for_timeout(ms)

    def cap(self, title="", sub=""):
        self.page.evaluate("([t, s]) => __cap(t, s)", [title, sub])

    def capbox(self, cx, maxw, bottom):
        self.page.evaluate("([a, b, c]) => __capbox(a, b, c)", [cx, maxw, bottom])

    def key(self, text="", x=None, y=None):
        self.page.evaluate("([t, x, y]) => __key(t, x, y)", [text, x, y])

    # ---- the cursor
    def move(self, x, y, steps=18):
        x0, y0 = self.x, self.y
        for i in range(1, steps + 1):
            t = i / steps
            e = t * t * (3 - 2 * t)
            cx, cy = x0 + (x - x0) * e, y0 + (y - y0) * e
            self.page.mouse.move(cx, cy)
            self.page.evaluate(f"__curAt({cx},{cy})")
            self.page.wait_for_timeout(16)
        self.x, self.y = x, y

    def box(self, frame, sel):
        b = self.frame(frame).locator(sel).first.bounding_box()
        return b["x"] + b["width"] / 2, b["y"] + b["height"] / 2

    def hover(self, frame, sel, steps=18):
        self.move(*self.box(frame, sel), steps=steps)

    def click(self, frame, sel, steps=18):
        self.hover(frame, sel, steps)
        self.page.evaluate("__down(true)")
        self.page.mouse.down()
        self.page.wait_for_timeout(90)
        self.page.mouse.up()
        self.page.evaluate("__down(false)")

    # ---- the GIF
    def finish(self, crop, out_w=720, fps=10, colors=112):
        video = self.page.video
        self.ctx.close()
        self.browser.close()
        src = Path(video.path())
        dur = float(subprocess.check_output(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(src)]).decode().strip())
        x, y, w, h = crop
        ss = max(0.0, (self.start or 0) - 0.1)
        vf = f"trim=start={ss:.2f},setpts=PTS-STARTPTS,crop={w}:{h}:{x}:{y},fps={fps},scale={out_w}:-1:flags=lanczos"
        OUT.mkdir(exist_ok=True)
        pal = TMP / f"{self.name}-pal.png"
        gif = OUT / f"{self.name}.gif"
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", str(src), "-vf", f"{vf},palettegen=max_colors={colors}:stats_mode=full", str(pal)], check=True)
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", str(src), "-i", str(pal), "-lavfi",
                        f"{vf}[v];[v][1:v]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle", str(gif)], check=True)
        print(f"{gif.name}: {os.path.getsize(gif)/1e6:.2f} MB, {dur - ss:.1f} s")


def extras_for(id_, gs, cp, items=True):
    return {"gear_score": gs, "combat_power": cp, "portrait": None, "items": ITEMS if items else [], "page": True, "gear_ready": items}


# The overlay (900 px: a 1280 x 720 screen makes it read at GIF size) and
# a strip of the game under it.
CROP = (170, 0, 940, 380)
CX = 640


def overview(pw):
    r = Rec(pw, "overview")
    r.capbox(CX, 700, H - 372)
    r.mark()
    r.cap("You die in a fight", "Death Recap opens at the top of the screen")
    r.wait(1400)
    r.recap()
    r.wait(1500)
    r.cap("Who killed you, and with what", "Their class, the final blow, their power against yours")
    r.hover("overlay", ".bar .name-line")
    r.wait(2400)
    r.cap("Everyone who hit you", "How long you could not act, and what you dealt back")
    r.hover("overlay", ".attackers")
    r.wait(1300)
    r.hover("overlay", ".cc")
    r.wait(1300)
    r.hover("overlay", ".dealt")
    r.wait(1600)
    r.cap("Back on your feet, it goes away")
    r.move(640, 560)
    r.emit("overlay", "revived")
    r.wait(1800)
    r.cap()
    r.wait(500)
    r.finish(CROP, fps=8, colors=96)


TALL = (170, 0, 940, 480)


def details(pw):
    r = Rec(pw, "details")
    r.recap()
    r.wait(800)
    r.capbox(CX, 700, H - 472)
    r.move(930, 300)
    r.mark()
    r.cap("Hold Alt for the details", "Or click the bar to keep them open")
    r.wait(1000)
    r.key("Alt", 380, 432)
    r.emit("overlay", "alt", True)
    r.wait(2000)
    r.cap("Each attacker, one line", "When they joined, damage and crits, their skills")
    r.hover("overlay", ".rows .row:nth-child(2) .dm")
    r.wait(1800)
    r.cap("What controlled you", "The ability, what it did, how long")
    r.hover("overlay", ".rows .row:nth-child(1) .ccc .eff")
    r.wait(2200)
    r.cap("Who hunts you", "The red number: they killed you before")
    r.hover("overlay", ".bar .kills", steps=24)
    r.wait(2200)
    r.cap()
    r.key()
    r.emit("overlay", "alt", False)
    r.move(930, 300)
    r.wait(700)
    r.finish(TALL, fps=8, colors=96)


def lookups(pw):
    r = Rec(pw, "lookups")
    r.recap()
    r.wait(800)
    r.capbox(CX, 700, H - 472)
    r.move(930, 300)
    r.mark()
    r.cap("Online lookups, if you turn them on", "Gear score and gear from the official AION 2 site")
    r.wait(1800)
    r.cap("The killer first", "Then whoever opened the fight, then the rest")
    r.extra(KILLER, extras_for(KILLER, 2718, 140596, items=False))
    r.wait(900)
    r.extra(OPENER, extras_for(OPENER, 2741, 151200, items=False))
    r.wait(700)
    r.extra(FOURTH, extras_for(FOURTH, 2690, 132800, items=False))
    r.wait(700)
    r.extra(THIRD, extras_for(THIRD, 2655, 128400, items=False))
    r.wait(900)
    r.extra(KILLER, extras_for(KILLER, 2718, 140596))
    r.cap("Hover their portrait for their gear", "Enchant level, and the diamond level past +15")
    r.hover("overlay", ".killer-portrait", steps=22)
    r.wait(3600)
    r.cap()
    r.move(930, 300)
    r.wait(700)
    r.finish(TALL)


def looks(pw):
    r = Rec(pw, "looks", bg="bg2.png")
    r.recap()
    r.wait(600)
    r.page.evaluate("document.getElementById('settings').style.display='block'")
    r.frame("settings").evaluate("window.dispatchEvent(new Event('focus'))")
    r.wait(800)
    r.capbox(CX, 700, H - 592)
    r.move(880, 520)
    r.mark()
    r.cap("Two looks", "The HUD every Flav-Toolkit add-on shares, or the game's own")
    r.wait(1800)
    r.click("settings", "#theme button[data-value='game']")
    r.wait(2600)
    r.click("settings", "#theme button[data-value='hud']")
    r.wait(2200)
    r.cap("Settings from Flav-Toolkit's hub", "Lookups, every fight, how long to remember")
    r.hover("settings", ".sw[data-key='every_fight']")
    r.wait(2200)
    r.cap()
    r.wait(500)
    r.finish((170, 0, 940, 600))


if __name__ == "__main__":
    which = sys.argv[1:] or ["overview", "details", "lookups", "looks"]
    prepare()
    srv = serve()
    with sync_playwright() as pw:
        for name in which:
            globals()[name](pw)
    srv.shutdown()
