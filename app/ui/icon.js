// The Death Recap icon: a click opens (or closes) the last recap, a drag
// moves the icon (Death Recap remembers where), a right-click opens the
// settings.

(() => {
  const tauri = window.__TAURI__;
  const invoke = tauri.core.invoke;
  const listen = tauri.event.listen;
  const win = tauri.window.getCurrentWindow();
  const icon = document.getElementById("icon");

  let down = null;
  icon.addEventListener("mousedown", (e) => {
    if (e.button !== 0) return;
    down = { x: e.screenX, y: e.screenY };
  });
  window.addEventListener("mousemove", (e) => {
    if (!down || !(e.buttons & 1)) return;
    if (Math.abs(e.screenX - down.x) + Math.abs(e.screenY - down.y) > 3) {
      down = null;
      win.startDragging().catch(() => {});
    }
  });
  window.addEventListener("mouseup", (e) => {
    if (e.button !== 0 || !down) return;
    down = null;
    invoke("toggle_recap").catch(() => {});
  });
  document.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    invoke("open_settings").catch(() => {});
  });

  listen("has-recap", () => icon.classList.remove("empty"));
  listen("nothing", () => {
    icon.classList.remove("nudge");
    void icon.offsetWidth;
    icon.classList.add("nudge");
  });
  listen("overlay-open", (e) => icon.classList.toggle("open", !!e.payload));
  invoke("last_recap").then((r) => { if (r) icon.classList.remove("empty"); }).catch(() => {});
})();
