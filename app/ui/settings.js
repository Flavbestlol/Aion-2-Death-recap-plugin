// Death Recap's settings, opened by the hub's Settings (or by starting Death
// Recap again). Every change is saved at once.

(() => {
  const tauri = window.__TAURI__;
  const invoke = tauri.core.invoke;
  const win = tauri.window.getCurrentWindow();

  function show(state) {
    const s = state.settings;
    for (const sw of document.querySelectorAll(".sw")) sw.setAttribute("aria-checked", s[sw.dataset.key] ? "true" : "false");
    for (const b of document.querySelectorAll("#history button")) b.setAttribute("aria-pressed", b.dataset.value === s.history ? "true" : "false");
    for (const b of document.querySelectorAll("#theme button")) b.setAttribute("aria-pressed", b.dataset.value === s.theme ? "true" : "false");
    document.body.dataset.theme = s.theme === "game" ? "game" : "hud";
    document.getElementById("version").textContent = state.version;
    const fights = state.fights === 1 ? "1 fight recorded" : `${state.fights} fights recorded`;
    document.getElementById("status").textContent = `${state.status} · ${fights}`;
  }
  const refresh = () => invoke("settings_get").then(show).catch(() => {});

  for (const sw of document.querySelectorAll(".sw")) {
    sw.addEventListener("click", () => {
      const on = sw.getAttribute("aria-checked") !== "true";
      invoke("settings_set", { change: { key: sw.dataset.key, on } }).then(show).catch(() => {});
    });
  }
  for (const key of ["history", "theme"]) {
    for (const b of document.querySelectorAll(`#${key} button`)) {
      b.addEventListener("click", () => invoke("settings_set", { change: { key, value: b.dataset.value } }).then(show).catch(() => {}));
    }
  }
  // × and Esc give the game its focus back; clicking another window only
  // closes the panel (the window clicked keeps the focus).
  const close = (giveBack) => invoke("settings_close", { giveBack }).catch(() => {});
  document.getElementById("close").addEventListener("click", () => close(true));
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") close(true); });
  win.onFocusChanged(({ payload: focused }) => { if (!focused) close(false); else refresh(); }).catch(() => {});
  document.addEventListener("contextmenu", (e) => e.preventDefault());
  // Changed in Flav-Toolkit's hub while the panel is open: show it.
  tauri.event.listen("settings-changed", refresh).catch(() => {});
  refresh();
})();
