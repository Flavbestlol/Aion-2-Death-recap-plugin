// A fake Tauri host for Death Recap's pages, for the README GIFs: answers
// invoke(), keeps listen() handlers, and lets the stage push events.
(() => {
  const top = window.top;
  if (window === top) {
    top.__zoom = 1;
    top.__state = top.__state || { recap: null, settings: { theme: "hud", online_lookups: true, every_fight: false, history: "day" } };
    return;
  }
  const frame = window.frameElement;
  const handlers = {};
  window.__calls = [];
  window.__emit = (name, payload) => { (handlers[name] || []).forEach((f) => f({ payload })); };
  const S = () => top.__state;
  const panel = () => ({ settings: S().settings, status: "Connected: Reading the game", version: "0.3.0", fights: 3 });
  const invoke = (cmd, args) => {
    window.__calls.push([cmd, args]);
    switch (cmd) {
      case "last_recap": return Promise.resolve(S().recap);
      case "settings_get": return Promise.resolve(panel());
      case "settings_set": {
        const c = args.change;
        if ("on" in c) S().settings[c.key] = c.on; else S().settings[c.key] = c.value;
        if (c.key === "theme") top.frames["overlay"].__emit("theme", c.value);
        return Promise.resolve(panel());
      }
      case "fit": frame.style.height = `${Math.ceil(args.height * (top.__zoom || 1))}px`; return Promise.resolve();
      case "hide_overlay": frame.style.display = "none"; return Promise.resolve();
      case "settings_close": frame.style.display = "none"; return Promise.resolve();
      default: return Promise.resolve(null);
    }
  };
  window.__TAURI__ = {
    core: { invoke },
    event: { listen: (name, f) => { (handlers[name] = handlers[name] || []).push(f); return Promise.resolve(() => {}); } },
    window: { getCurrentWindow: () => ({ onFocusChanged: () => Promise.resolve(() => {}) }) },
  };
  // The fonts Windows has, by free look-alikes (Bahnschrift, Segoe UI).
  const css = `@font-face{font-family:"Bahnschrift";font-weight:400 500;src:url(/fonts/BarlowSemiCondensed-Medium.ttf)}
@font-face{font-family:"Bahnschrift";font-weight:600;src:url(/fonts/BarlowSemiCondensed-SemiBold.ttf)}
@font-face{font-family:"Bahnschrift";font-weight:700 900;src:url(/fonts/BarlowSemiCondensed-Bold.ttf)}
@font-face{font-family:"Segoe UI Variable Text";font-weight:100 900;src:url(/fonts/OpenSans.ttf)}
@font-face{font-family:"Segoe UI";font-weight:100 900;src:url(/fonts/OpenSans.ttf)}`;
  document.addEventListener("DOMContentLoaded", () => {
    const s = document.createElement("style"); s.textContent = css; document.head.prepend(s);
    // A little larger than in game, so it reads at GIF size.
    document.documentElement.style.zoom = String(top.__zoom || 1);
  });
})();
