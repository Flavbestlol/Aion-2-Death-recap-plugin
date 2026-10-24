// The Death Recap overlay: shows what the reader sends ("recap", then
// "extra" for each attacker the official site describes, as it comes), drops
// the details down while Alt is held, and hides when you are back on your
// feet. Every name and number goes in as text, never as markup.

(() => {
  const tauri = window.__TAURI__;
  const invoke = tauri.core.invoke;
  const listen = tauri.event.listen;

  const RES = "https://assets.playnccdn.com/static-aion2-gamedata/resources";
  const SITE = "https://assets.playnccdn.com/static-aion2/characters/img/info";
  const CP_ICON = `${SITE}/profile_power_icon_pc.png`;
  const GS_ICON = `${SITE}/profile_level_icon_pc.png`;
  const CC_ICON = `${RES}/ICON_Abnormal_Stun.png`;
  // The official site's colours for item grades.
  const GRADE = { Common: "#5C6168", Rare: "#2F6B37", Legend: "#2C5F8E", Unique: "#8A6A2A", Epic: "#8E4A25", Mythic: "#7A2626", Special: "#1F6B5A" };

  const root = document.getElementById("recap");
  const $ = (sel, el = root) => el.querySelector(sel);
  let recap = null;
  let hideTimer = null;
  // The end of the fade-out; a recap shown meanwhile cancels it.
  let leaveTimer = null;
  // The details are open while Alt is held, or after a click (until the next).
  let altHeld = false;
  let pinned = false;
  let closingUntil = 0;
  const KIND = { skill: "Skill", passive: "Passive", theostone: "Theostone", attack: "Basic attack" };
  // The overlay's width (as main.rs makes it), how long a lost duel's and a
  // survived fight's recap stay, and the fade-out.
  const WIDTH = 900;
  const DUEL_SHOWN_MS = 20000;
  const FIGHT_SHOWN_MS = 12000;
  const FADE_MS = 260;

  // ---- helpers -----------------------------------------------------------
  const h = (tag, props = {}, ...kids) => {
    const el = document.createElement(tag);
    for (const [k, v] of Object.entries(props)) {
      if (v == null || v === false) continue;
      if (k === "class") el.className = v;
      else if (k === "text") el.textContent = v;
      // Through the CSSOM, so the page needs no inline styles in its CSP.
      else if (k === "style") el.style.cssText = v;
      else if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
      else el.setAttribute(k, v);
    }
    for (const kid of kids.flat()) if (kid != null) el.append(kid);
    return el;
  };
  const int = (n) => (n ?? 0).toLocaleString("en-US");
  const short = (n) => (n >= 1000 ? `${(n / 1000).toFixed(1)}K` : String(n));
  const secs = (ms) => `${(ms / 1000).toFixed(ms < 1000 ? 2 : 1)} s`;
  const pct = (p) => (p == null ? "" : `${p > 0 ? "+" : p < 0 ? "-" : "±"}${Math.abs(p)}%`);
  const diff = (d) => (d == null ? "" : `${d > 0 ? "+" : d < 0 ? "-" : "±"}${int(Math.abs(d))}`);
  const cls = (d) => (d == null ? "" : d > 0 ? "up" : "down");
  const img = (src, alt, cl) => {
    // No address: nothing to show (an empty src shows a broken image).
    if (!src) return h("span", { class: `${cl || ""} noimg`.trim() });
    const el = h("img", { src, alt: alt || "", class: cl, title: alt || null });
    el.addEventListener("error", () => { el.style.visibility = "hidden"; });
    return el;
  };
  const skillIcon = (ref, size) =>
    ref?.icon ? img(ref.icon, ref.name) : h("div", { class: "glyph", title: ref?.name || "Attack", text: "⚔", style: size ? `width:${size}px;height:${size}px` : null });
  const classIcon = (code) => (code ? `img/class/${code}.png` : "");
  const extraOf = (id) => recap?.extras?.[id] || {};
  const attacker = (id) => recap?.view.attackers.find((a) => a.id === id);
  // Who killed you, then who opened the fight (the earliest to join), then
  // the rest by damage: the order of the faces on the bar, and the order the
  // lookups go in (RecapView::by_importance).
  const byImportance = (v) => {
    const k = v.attackers.findIndex((a) => a.killer);
    let o = -1;
    v.attackers.forEach((a, i) => {
      if (i !== k && a.joined_ms != null && (o < 0 || a.joined_ms < v.attackers[o].joined_ms)) o = i;
    });
    const first = [k, o].filter((i) => i >= 0);
    return [...first.map((i) => v.attackers[i]), ...v.attackers.filter((_, i) => !first.includes(i))];
  };

  // Portraits come from the official site's image server, which sometimes
  // refuses when many are asked at once. Each is loaded aside; until it is
  // in, the class emblem stands in, and a refused one is asked again a few
  // seconds later, then less often, until it comes.
  const portraits = new Map();
  const RETRY_S = [2, 4, 8, 15, 30, 60, 120];
  function portraitReady(url) {
    const state = portraits.get(url);
    if (state === "ok") return true;
    if (state == null) loadPortrait(url, 0);
    return false;
  }
  function loadPortrait(url, n) {
    portraits.set(url, "waiting");
    const im = new Image();
    im.onload = () => { portraits.set(url, "ok"); rerender(); };
    im.onerror = () => {
      if (n < RETRY_S.length) setTimeout(() => loadPortrait(url, n + 1), RETRY_S[n] * 1000);
      else portraits.set(url, "failed");
    };
    im.src = url;
  }
  let rerenderAt = 0;
  function rerender() {
    if (rerenderAt) return;
    rerenderAt = requestAnimationFrame(() => {
      rerenderAt = 0;
      if (!recap) return;
      renderBar();
      renderRows();
      refreshGear();
      fit();
    });
  }

  // A monster's mark: the game has no monster portraits Death Recap can
  // show, so claw marks, and horns with a gold frame for a boss.
  const SVG = "http://www.w3.org/2000/svg";
  const monsterFace = (a) => {
    const boss = a.monster?.boss;
    const svg = document.createElementNS(SVG, "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("aria-hidden", "true");
    const p = document.createElementNS(SVG, "path");
    p.setAttribute("d", boss
      ? "M4 3c.5 3.5 2 5.5 4.2 6.4M20 3c-.5 3.5-2 5.5-4.2 6.4M12 7.5c-3.6 0-6 2.5-6 6 0 2.2 1 4 2.6 5v2.5h6.8V18.5c1.6-1 2.6-2.8 2.6-5 0-3.5-2.4-6-6-6zM9.6 13.6h.01M14.4 13.6h.01"
      : "M7 4l-2 16M12.5 4l-2 16M18 4l-2 16");
    p.setAttribute("stroke", "currentColor");
    p.setAttribute("stroke-width", boss ? "1.8" : "2.4");
    p.setAttribute("stroke-linecap", "round");
    p.setAttribute("stroke-linejoin", "round");
    p.setAttribute("fill", "none");
    svg.append(p);
    return h("div", { class: `face mon${boss ? " boss" : ""}`, title: boss ? `${a.name} (boss)` : a.name }, svg);
  };
  const face = (a, cl) => {
    const ex = extraOf(a.id);
    if (ex.portrait && portraitReady(ex.portrait)) return img(ex.portrait, a.name, "face");
    if (a.monster) return monsterFace(a);
    const el = img(classIcon(a.class_code), a.class || a.name, "face emblem");
    if (cl) el.classList.add(cl);
    return el;
  };
  const pvp = () => {
    const ns = "http://www.w3.org/2000/svg";
    const svg = document.createElementNS(ns, "svg");
    svg.setAttribute("class", "pvp");
    svg.setAttribute("viewBox", "0 0 24 20");
    svg.setAttribute("aria-hidden", "true");
    const p = document.createElementNS(ns, "path");
    p.setAttribute("d", "M3 2l9 9M21 2l-9 9M6.5 14.5L2.5 18.5M17.5 14.5l4 4M5 12l3 3M19 12l-3 3");
    p.setAttribute("stroke", "#EB3A34");
    p.setAttribute("stroke-width", "2.6");
    p.setAttribute("stroke-linecap", "round");
    p.setAttribute("fill", "none");
    svg.append(p);
    return svg;
  };
  const gsOf = (a) => extraOf(a.id).gear_score ?? a.gear_score ?? null;

  // ---- the bar -----------------------------------------------------------
  function renderBar() {
    const v = recap.view;
    const killerId = v.final_blow?.actor;
    const k = attacker(killerId) || v.attackers[0];
    const kp = $(".killer-portrait");
    kp.dataset.gearFor = k ? k.id : "";
    kp.classList.toggle("killed", !!v.final_blow);
    const oldFace = $(".killer-portrait .face");
    if (k) oldFace.replaceWith(face(k));
    $(".killer-portrait .pvp").style.display = v.final_blow && k?.player ? "" : "none";
    const kills = $(".kills");
    // Shown from the second time: this death is counted in it.
    kills.hidden = !(k && k.kills_on_you > 1);
    kills.textContent = k ? String(k.kills_on_you) : "";
    kills.dataset.name = k ? k.name : "";
    const classImg = $(".name-line .class");
    classImg.hidden = !k?.class_code;
    if (k?.class_code) { classImg.src = classIcon(k.class_code); classImg.alt = k.class || ""; }
    const name = $(".bar .name");
    name.textContent = k ? k.name : "—";
    name.dataset.id = k ? k.id : "";
    name.classList.toggle("can", !!(k && extraOf(k.id).page));
    const blowIcon = $(".blow-icon");
    if (v.final_blow?.skill.icon) { blowIcon.src = v.final_blow.skill.icon; blowIcon.style.display = ""; }
    else blowIcon.style.display = "none";
    blowIcon.title = v.final_blow ? `Final blow: ${v.final_blow.skill.name}` : "";
    $(".blow-dmg").textContent = v.final_blow ? int(v.final_blow.damage) + (v.final_blow.crit ? " crit" : "") : "";

    // A monster has no combat power or gear: it says what it is instead.
    const mon = $(".bar .what-tag");
    mon.hidden = !k?.monster;
    mon.textContent = k?.monster ? (k.monster.boss ? "Boss" : "Monster") : "";
    mon.classList.toggle("boss", !!k?.monster?.boss);
    $(".bar .cp").hidden = !!k?.monster;
    const cp = $(".bar .cp"), gs = $(".bar .gs");
    $(".v", cp).textContent = k?.combat_power != null ? short(k.combat_power) : "—";
    const d = $(".d", cp); d.textContent = pct(k?.combat_power_vs_me); d.className = `d ${cls(k?.combat_power_vs_me)}`;
    cp.title = `Combat power${v.me.combat_power ? ` (yours ${short(v.me.combat_power)})` : ""}`;
    const g = k ? gsOf(k) : null;
    gs.hidden = g == null || !!k?.monster;
    $(".v", gs).textContent = g != null ? int(g) : "";
    const gd = g != null && v.me.gear_score != null ? g - v.me.gear_score : null;
    const gdEl = $(".d", gs); gdEl.textContent = diff(gd); gdEl.className = `d ${cls(gd)}`;
    gs.title = `Gear score${v.me.gear_score ? ` (yours ${int(v.me.gear_score)})` : ""}`;

    const faces = $(".faces");
    faces.replaceChildren(...byImportance(v).slice(0, 4).map((a) => face(a)));
    $(".attackers .count").textContent = `×${v.attackers.length}`;
    $(".attackers .dmg").textContent = `${int(v.damage)} · ${secs(v.duration_ms)}`;
    $(".cc-time").textContent = v.cc_ms > 0 ? secs(v.cc_ms) : "0 s";
    $(".cc-count").textContent = `CC ×${v.cc_count}`;
    const top = ccLines().slice(0, 3);
    $(".cc-from").replaceChildren(...top.map((l) => effIcon(l.e, null)));

    $(".dealt .n").textContent = int(v.you_dealt);
    const toK = k && k.you_dealt > 0 ? ` · ${short(k.you_dealt)} to ${k.name}` : "";
    $(".dealt .to").textContent = `you dealt${toK}`;
    $(".dealt").title = v.you_hits ? `You dealt ${int(v.you_dealt)} in ${v.you_hits} hit${v.you_hits === 1 ? "" : "s"} in this fight (from 10 s before it)` : "You dealt no damage in this fight";
  }

  // ---- crowd control: which ability, from whom -----------------------------
  // Every CC on you, one line per effect and the ability that put it.
  function ccLines() {
    const lines = [];
    for (const a of recap.view.attackers) for (const e of a.cc) lines.push({ a, e });
    return lines.sort((x, y) => y.e.total_ms - x.e.total_ms || y.e.count - x.e.count);
  }
  // The ability's icon, the kind of CC in its corner, how many times under it.
  function effIcon(e, key, minor) {
    const src = e.sources[0];
    const main = src?.icon || e.icon;
    const badge = src?.icon && e.icon && e.icon !== src.icon ? img(e.icon, e.name, "badge") : null;
    return h("span", { class: `eff${minor ? " minor" : ""}`, "data-cc": key },
      main ? img(main, src ? src.name : e.name) : h("div", { class: "glyph", text: "⚔" }),
      badge, e.count > 1 ? h("b", { text: String(e.count) }) : null);
  }
  function ccLine({ a, e }) {
    const src = e.sources[0];
    return h("div", { class: "cl" },
      effIcon(e, null),
      h("div", { class: "what" },
        h("div", {}, h("span", { class: "ab", text: src ? src.name : e.name }), src ? h("span", { class: "kind", text: KIND[src.kind] || src.kind }) : null),
        h("div", { class: "how" }, h("span", { class: "fx", text: `${e.name}${e.count > 1 ? ` ×${e.count}` : ""}` }), e.total_ms > 0 ? h("span", { text: ` · ${secs(e.total_ms)}` }) : null, a ? h("span", { text: ` · ${a.name}` }) : null)));
  }
  function showCc(el) {
    const card = $(".ccard");
    let lines;
    if (el.dataset.cc === "all") lines = ccLines();
    else {
      const [id, i] = el.dataset.cc.split(":").map(Number);
      const a = attacker(id);
      const e = a && (a.cc[i] || a.debuffs[i - a.cc.length]);
      lines = e ? [{ a, e }] : [];
    }
    if (!lines.length) return;
    const head = el.dataset.cc === "all" ? h("div", { class: "t", text: "What controlled you" }) : null;
    card.replaceChildren(...[head, ...lines.slice(0, 8).map(ccLine)].filter(Boolean));
    const r = el.getBoundingClientRect();
    card.style.left = `${Math.max(4, Math.min(r.left - 8, WIDTH - 300))}px`;
    card.style.top = `${r.bottom + 6}px`;
    card.hidden = false;
    fit();
  }

  // ---- the details (Alt) -------------------------------------------------
  function renderRows() {
    const v = recap.view;
    const rows = v.attackers.slice(0, 6).map((a) => {
      const g = gsOf(a);
      const gd = g != null && v.me.gear_score != null ? g - v.me.gear_score : null;
      const port = h("div", { class: `portrait${a.killer ? " killed" : ""}`, "data-gear-for": a.id }, face(a), a.killer && a.player ? pvp() : null);
      const joined = a.joined_ms === 0
        ? h("span", { class: "joined opened", title: a.opener ? `First hit: ${a.opener.name}` : null, text: "opened the fight" })
        : a.joined_ms != null ? h("span", { class: "joined", title: a.opener ? `First hit: ${a.opener.name}` : null, text: `joined +${(a.joined_ms / 1000).toFixed(1)} s` }) : null;
      const who = h("div", { class: "who-cell" }, port,
        h("div", { class: "who" },
          h("span", { class: "name-line" }, a.class_code ? img(classIcon(a.class_code), a.class, "class") : null,
            h("button", { type: "button", class: `name link${extraOf(a.id).page ? " can" : ""}`, "data-id": a.id, text: a.name })),
          joined));
      const pw = a.monster ? h("div", { class: "pw" }, h("span", { class: `what-tag${a.monster.boss ? " boss" : ""}`, text: a.monster.boss ? "Boss" : "Monster" })) : h("div", { class: "pw" },
        h("span", { class: "stat" }, img(CP_ICON, "Combat power", "i-cp"), h("span", { style: "color:var(--cp)", text: a.combat_power != null ? short(a.combat_power) : "—" }), h("span", { class: `d ${cls(a.combat_power_vs_me)}`, text: pct(a.combat_power_vs_me) })),
        g != null ? h("span", { class: "stat" }, img(GS_ICON, "Gear score", "i-gs"), h("span", { text: int(g) }), h("span", { class: `d ${cls(gd)}`, text: diff(gd) })) : null);
      const dm = h("div", { class: "dm" }, h("span", { class: "n", text: int(a.damage) }),
        h("div", { class: "bar-crit", title: `${a.crit_share}% of it from crits` }, h("div", { style: `width:${a.crit_share}%` })),
        h("span", { class: "c", text: `${a.crit_share}% crit · ${a.hits} hits` }),
        a.you_dealt > 0 ? h("span", { class: "you", title: `What you dealt to ${a.name} in this fight`, text: `you: ${int(a.you_dealt)}` }) : null);
      const finalSkill = a.killer ? v.final_blow?.skill.name : null;
      const shown = a.skills.slice(0, 6);
      const sk = h("div", { class: "sk" }, shown.map((s) => h("figure", { class: s.skill.name === finalSkill ? "final" : null, title: `${s.skill.name}: ${int(s.damage)} in ${s.hits} hit${s.hits === 1 ? "" : "s"}${s.specs.length ? ` · specialties ${s.specs.join("+")}` : ""}` },
        skillIcon(s.skill, 28), h("figcaption", { text: short(s.damage) }))),
        a.skills.length > shown.length ? h("span", { class: "more", text: `+${a.skills.length - shown.length}` }) : null);
      const cc = a.cc.slice(0, 4).map((e, i) => effIcon(e, `${a.id}:${i}`));
      const rest = a.debuffs.slice(0, 2).map((e, i) => effIcon(e, `${a.id}:${a.cc.length + i}`, true));
      const ccc = h("div", { class: "ccc" }, cc.length ? cc : h("span", { class: "none", text: "no CC" }), a.cc_ms > 0 ? h("span", { class: "t", text: secs(a.cc_ms) }) : null, rest.length ? h("span", { class: "rest" }, rest) : null);
      return h("div", { class: "row" }, who, pw, dm, sk, ccc);
    });
    if (v.attackers.length > 6) rows.push(h("div", { class: "row more", text: `+${v.attackers.length - 6} more attackers` }));
    $(".rows").replaceChildren(...rows);

    const foot = [];
    if (v.mobs) {
      const before = v.mobs.before_players_ms ? ` for ${Math.round(v.mobs.before_players_ms / 1000)} s before` : "";
      foot.push(h("span", { class: "mobs", text: `Mobs: ${int(v.mobs.damage)}${before}` }));
    }
    if (v.avoided.length) {
      foot.push(h("span", { text: "Resisted" }));
      for (const m of v.avoided.slice(0, 8)) foot.push(m.skill.icon ? img(m.skill.icon, `You ${m.resisted ? "resisted" : "dodged"} ${m.skill.name}`) : null);
    }
    if (v.after_end_damage > 0) foot.push(h("span", { title: "Hits already on their way when it ended", text: `+${int(v.after_end_damage)} after` }));
    if (v.me.combat_power != null || v.me.gear_score != null) {
      foot.push(h("span", { class: "me" }, h("span", { text: "You" }),
        v.me.combat_power != null ? img(CP_ICON, "Combat power", "i-cp") : null,
        v.me.combat_power != null ? h("span", { style: "color:var(--cp)", text: short(v.me.combat_power) }) : null,
        v.me.gear_score != null ? img(GS_ICON, "Gear score", "i-gs") : null,
        v.me.gear_score != null ? h("span", { text: int(v.me.gear_score) }) : null));
    }
    $(".foot").replaceChildren(...foot.filter(Boolean));
  }

  // ---- hover cards -------------------------------------------------------
  // A diamond past +15: the official site's blue mark with its level.
  const diamond = (n) => h("span", { class: "ex", title: `Diamond ${n}` }, h("b", { text: String(n) }));
  const itemTitle = (it) => `${it.name} +${it.enchant}${it.exceed ? ` · Diamond ${it.exceed}` : ""} · ${it.grade}`;
  let gearFor = null;
  function showGear(el) {
    const id = Number(el.dataset.gearFor);
    const a = attacker(id);
    const ex = extraOf(id);
    const card = $(".gear");
    // Nothing from the official site (yet): no card.
    if (!a || (ex.combat_power == null && !(ex.items && ex.items.length))) { card.hidden = true; gearFor = null; return; }
    gearFor = id;
    const g = ex.gear_score;
    const items = (ex.items || []).filter((it) => !/^Arcana|^Rune/.test(it.slot)).slice(0, 18);
    const note = items.length ? "From the official AION 2 site" : ex.gear_ready ? "No gear on the official AION 2 site" : "Gear on its way from the official AION 2 site…";
    card.replaceChildren(...[
      h("div", { class: "head" }, face(a),
        h("div", {}, h("div", { class: "n", text: a.name }),
          h("div", { class: "s" },
            h("span", {}, img(CP_ICON, "Combat power", "i-cp"), h("span", { text: int(ex.combat_power ?? a.combat_power) })),
            g != null ? h("span", {}, img(GS_ICON, "Gear score", "i-gs"), h("span", { text: int(g) })) : null))),
      items.length ? h("div", { class: "grid" }, items.map((it) =>
        h("div", { class: "it", style: `background:${GRADE[it.grade] || "#3A3F47"}`, title: itemTitle(it) },
          img(it.icon, it.name), it.exceed ? diamond(it.exceed) : null, it.enchant ? h("span", { class: "en", text: `+${it.enchant}` }) : null))) : null,
      h("div", { class: `note${items.length || ex.gear_ready ? "" : " waiting"}`, text: note })].filter(Boolean));
    const r = el.getBoundingClientRect();
    card.style.left = `${Math.min(r.left, WIDTH - 310)}px`;
    card.style.top = `${r.bottom + 6}px`;
    card.hidden = false;
    fit();
  }
  // The open gear card again, when more came in for it.
  function refreshGear() {
    if (gearFor == null || $(".gear").hidden) return;
    const el = root.querySelector(`[data-gear-for="${gearFor}"]`);
    if (el) showGear(el);
  }

  function showHistory(el) {
    const kills = recap?.history?.[el.dataset.name] || [];
    const card = $(".hist");
    if (!kills.length) return;
    card.replaceChildren(h("div", {}, h("b", { text: `${el.dataset.name} killed you` }), h("span", { class: "t", text: `×${kills.length}` })),
      ...kills.map((k) => h("div", {}, h("span", { text: new Date(k.at_ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) + (k.duel ? " · duel" : "") }), h("span", { class: "t", text: k.skill || "" }))));
    const r = el.getBoundingClientRect();
    card.style.left = `${r.left}px`;
    card.style.top = `${r.bottom + 6}px`;
    card.hidden = false;
    fit();
  }

  root.addEventListener("mouseover", (e) => {
    const p = e.target.closest("[data-gear-for]");
    if (p && p.dataset.gearFor) showGear(p);
    const k = e.target.closest(".kills");
    if (k) showHistory(k);
    const c = e.target.closest("[data-cc]");
    if (c && c.dataset.cc) showCc(c);
  });
  root.addEventListener("mouseout", (e) => {
    if (e.target.closest("[data-gear-for]")) { $(".gear").hidden = true; gearFor = null; fit(); }
    if (e.target.closest(".kills")) { $(".hist").hidden = true; fit(); }
    if (e.target.closest("[data-cc]")) { $(".ccard").hidden = true; fit(); }
  });
  root.addEventListener("click", (e) => {
    const n = e.target.closest(".name.can");
    if (n && recap) return invoke("open_profile", { fightId: recap.fight_id, id: Number(n.dataset.id) });
    if (e.target.closest(".close")) return hide();
    // The Alt key, or anywhere else on the bar: the details open or close.
    if (e.target.closest(".key") || (e.target.closest(".bar") && !e.target.closest("button, .portrait, .kills"))) {
      pinned = !isOpen();
      applyOpen();
    }
  });

  // ---- size, show, hide --------------------------------------------------
  // The window is as tall as the bar, the details when open (or closing),
  // and any hover card: worked out from what is shown, never measured
  // mid-slide, so a quick Alt never leaves the window cut short.
  const isOpen = () => altHeld || pinned;
  function height() {
    let bottom = $(".bar").offsetHeight;
    if (isOpen() || Date.now() < closingUntil) bottom += $(".drop-inner").scrollHeight;
    for (const card of root.querySelectorAll(".gear:not([hidden]), .hist:not([hidden]), .ccard:not([hidden])")) bottom = Math.max(bottom, card.offsetTop + card.offsetHeight);
    return Math.ceil(bottom) + 2;
  }
  function fit() {
    requestAnimationFrame(() => { if (!root.hidden) invoke("fit", { height: height() }); });
  }
  function applyOpen() {
    const open = isOpen();
    $(".key").classList.toggle("on", open);
    if (open === root.classList.contains("open")) return fit();
    root.classList.toggle("open", open);
    if (!open) {
      // The details slide shut in 320 ms (style.css): the window keeps their
      // height until then.
      closingUntil = Date.now() + 360;
      setTimeout(fit, 380);
    }
    fit();
  }
  function show(r) {
    recap = r;
    clearTimeout(hideTimer);
    clearTimeout(leaveTimer);
    pinned = false;
    root.classList.remove("leaving", "open");
    $(".key").classList.remove("on");
    renderBar();
    renderRows();
    root.hidden = false;
    fit();
    // A death stays until you are back; a lost duel or a survived fight goes by itself.
    if (r.view.outcome !== "died") hideTimer = setTimeout(hide, r.view.outcome === "duel_lost" ? DUEL_SHOWN_MS : FIGHT_SHOWN_MS);
  }
  function hide() {
    pinned = false;
    root.classList.add("leaving");
    leaveTimer = setTimeout(() => { root.hidden = true; root.classList.remove("open"); invoke("hide_overlay"); }, FADE_MS);
  }

  listen("recap", (e) => show(e.payload));
  listen("extra", (e) => {
    const m = e.payload;
    if (!recap || recap.fight_id !== m.fight_id) return;
    recap.extras = recap.extras || {};
    recap.extras[m.id] = m.extra;
    rerender();
  });
  listen("alt", (e) => {
    altHeld = !!e.payload;
    if (recap && !root.hidden) applyOpen();
  });
  // Windows does not let Alt be read while the game runs as administrator
  // and Death Recap does not: the key then says to click.
  listen("alt-blocked", (e) => {
    const key = $(".key");
    key.textContent = e.payload ? "Click" : "Alt";
    key.title = e.payload
      ? "Alt can't be read while the game runs as administrator: click here for the details, or run Flav-Toolkit as administrator (it starts Death Recap)"
      : "Hold Alt, or click, for the details";
  });
  listen("revived", () => { if (recap && !root.hidden && recap.view.outcome === "died") hide(); });

  // The look: the HUD (every add-on's) or the game's own; the settings say.
  const setTheme = (t) => { document.body.dataset.theme = t === "game" ? "game" : "hud"; fit(); };
  listen("theme", (e) => setTheme(e.payload));
  invoke("settings_get").then((st) => setTheme(st.settings.theme)).catch(() => {});

  // The static icons.
  for (const el of root.querySelectorAll(".i-cp")) el.src = CP_ICON;
  for (const el of root.querySelectorAll(".i-gs")) el.src = GS_ICON;
  $(".i-cc").src = CC_ICON;
  invoke("last_recap").then((r) => { if (r) show(r); }).catch(() => {});
})();
