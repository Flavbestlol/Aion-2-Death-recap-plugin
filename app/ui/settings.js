// Death Recap's settings, opened by a right-click on the skull icon (or by
// starting Death Recap again). Every change is saved at once.

(() => {
  const tauri = window.__TAURI__;
  const invoke = tauri.core.invoke;
  const win = tauri.window.getCurrentWindow();

  function show(state) {
    const s = state.settings;
    for (const sw of document.querySelectorAll(".sw")) sw.setAttribute("aria-checked", s[sw.dataset.key] ? "true" : "false");
    for (const b of document.querySelectorAll("#history button")) b.setAttribute("aria-pressed", b.dataset.value === s.history ? "true" : "false");
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
  for (const b of document.querySelectorAll("#history button")) {
    b.addEventListener("click", () => invoke("settings_set", { change: { key: "history", value: b.dataset.value } }).then(show).catch(() => {}));
  }
  const close = () => invoke("settings_close").catch(() => {});
  document.getElementById("close").addEventListener("click", close);
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
  // Clicking elsewhere closes it.
  win.onFocusChanged(({ payload: focused }) => { if (!focused) close(); else refresh(); }).catch(() => {});
  document.addEventListener("contextmenu", (e) => e.preventDefault());
  refresh();
})();
