// Kubeport site. Shared by the English and French pages: every text that this script
// writes comes from the page's <script type="application/json" id="i18n"> dictionary.
const T = JSON.parse(document.getElementById("i18n").textContent);
const fmt = (text, ...args) => text.replace(/\{(\d)\}/g, (_, i) => args[i]);
const dur = (minutes) => (minutes % 60 === 0 ? fmt(T.durHour, minutes / 60) : fmt(T.durMin, minutes));
const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;

// The language switch points at the same section of the other page.
const altLinks = document.querySelectorAll("[data-alt]");
const syncAlt = () => altLinks.forEach((a) => { a.href = a.dataset.alt + location.hash; });
syncAlt();
addEventListener("hashchange", syncAlt);

// Copy a command block. If the browser refuses, select the text so Cmd+C works.
const copyStatus = document.getElementById("copy-status");
document.querySelectorAll("button.copy").forEach((button) => {
  const label = button.textContent;
  button.addEventListener("click", async () => {
    const block = document.getElementById(button.dataset.copy);
    let message = T.copied;
    try {
      await navigator.clipboard.writeText(block.innerText);
    } catch {
      const range = document.createRange();
      range.selectNodeContents(block);
      const selection = getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
      message = T.copyFailed;
    }
    button.textContent = message;
    copyStatus.textContent = message;
    setTimeout(() => { button.textContent = label; copyStatus.textContent = ""; }, 2500);
  });
});

// ---------------------------------------------------------------------------
// A loop of timed steps that runs only while its element is on screen, the tab is
// visible, the visitor has not paused it and it has not been stopped for good.
// events: [[ms, fn]] after the start; onStart runs at ms 0 of every cycle.
// ---------------------------------------------------------------------------
function makeLoop({ events, period, onStart, onPause, container }) {
  let pos = 0, timer = 0, startedAt = 0, playing = false, onscreen = false, userPaused = false, stopped = false;
  function schedule() {
    clearTimeout(timer);
    const next = events.find(([t]) => t > pos);
    const at = next ? next[0] : period;
    startedAt = performance.now();
    timer = setTimeout(() => {
      if (next) { pos = at; next[1](); } else { pos = 0; onStart(); }
      schedule();
    }, at - pos);
  }
  function sync() {
    const want = onscreen && !document.hidden && !userPaused && !stopped;
    if (container) container.classList.toggle("paused", !want);
    if (want === playing) return;
    playing = want;
    if (want) {
      schedule();
    } else {
      clearTimeout(timer);
      pos = Math.min(pos + performance.now() - startedAt, period - 1);
      if (onPause) onPause();
    }
  }
  return {
    observe(el, threshold) {
      new IntersectionObserver((entries) => {
        onscreen = entries[entries.length - 1].isIntersecting;
        sync();
      }, { threshold }).observe(el);
      document.addEventListener("visibilitychange", sync);
    },
    pause(value) { userPaused = value; sync(); },
    stop() { stopped = true; sync(); },
  };
}
function wirePause(button, loop) {
  if (reduced) { button.hidden = true; return; }
  let paused = false;
  button.addEventListener("click", () => {
    paused = !paused;
    button.textContent = paused ? T.play : T.pause;
    loop.pause(paused);
  });
}

// ---------------------------------------------------------------------------
// The panel in the hero: a working copy of the app's window.
// Rules mirror the app: the master switch is on while any Forward is Active; off
// deactivates all and remembers the set; on brings that set back (disabled when there
// is nothing to bring back). The menu bar icon: outline hexagon when nothing is on,
// filled when something is, filled with a red dot when a Forward is in Error.
// ---------------------------------------------------------------------------
const wrap = document.getElementById("panelwrap");
const panel = document.getElementById("panel");
const icon = document.getElementById("mb-icon");
const master = document.getElementById("master");
const disc = document.getElementById("disc");
const inactiveBox = document.getElementById("inactive-box");
const lists = { true: document.getElementById("active-list"), false: document.getElementById("inactive-list") };
const panelSay = document.getElementById("panel-status");
let silent = false; // the automatic demo does not talk to screen readers
const say = (text) => { if (!silent) panelSay.textContent = text; };

const forwards = [];
let idSeq = 0;
function track(row) {
  const sw = row.querySelector(".sw");
  const active = sw.getAttribute("aria-checked") === "true";
  const f = {
    id: row.dataset.id, row, sw, order: forwards.length, active,
    port: row.querySelector(".port").textContent.slice(1),
    state: active ? "connected" : null,
    fails: row.dataset.fails === "1",
    chip: row.querySelector(".chip"),
    reason: row.querySelector(".reason"),
    det: row.querySelector(".details"),
    btn: row.querySelector(".rbtn"),
    podEls: row.querySelectorAll(".d-pod"),
    podVal: row.querySelector(".d-pod-v"),
    idleEls: row.querySelectorAll(".d-idle"),
    idleVal: row.querySelector(".d-idle-v"),
    open: false, demoReason: null, demoIdle: null, idleSetting: "inherit",
    settleTimer: 0, moveTimer: 0,
  };
  f.podName = f.podVal.textContent;
  f.pod0 = f.podName;
  forwards.push(f);
  return f;
}
panel.querySelectorAll(".row").forEach(track);
const byId = (id) => forwards.find((f) => f.id === id);
const template = forwards[0].row.cloneNode(true);
template.querySelector(".sw").classList.remove("nudge");
let saved = null; // the set the master switch brings back

const STATE_LABEL = { connected: T.stateConnected, reconnecting: T.stateReconnecting, error: T.stateError, inactive: T.stateInactive };

// The delay shown in a row's details: the default of Settings, or the Forward's own choice.
function idleMinutes(f) {
  if (f.demoIdle) return f.demoIdle;
  const setting = f.idleSetting === "inherit" ? idleSelect.value : f.idleSetting;
  return setting === "never" ? 0 : Number(setting);
}

function paint(f) {
  const s = f.active ? f.state : "inactive";
  f.sw.setAttribute("aria-checked", String(f.active));
  f.row.dataset.active = String(f.active);
  f.chip.dataset.s = s;
  f.chip.setAttribute("aria-label", STATE_LABEL[s]);
  const reasonText = f.demoReason || (f.fails && s === "error" ? T.reasonBilling : "");
  f.reason.hidden = !reasonText;
  f.reason.textContent = reasonText;
  f.reason.dataset.k = f.demoReason ? "warn" : "err";
  const connected = f.active && s === "connected";
  f.podEls.forEach((e) => { e.hidden = !connected; });
  f.podVal.textContent = f.podName;
  const minutes = connected ? idleMinutes(f) : 0;
  f.idleEls.forEach((e) => { e.hidden = !minutes; });
  if (minutes) f.idleVal.textContent = fmt(T.idleCountdown, dur(minutes));
}

function toggle(f, open = !f.open) {
  f.open = open;
  f.det.hidden = !open;
  f.btn.setAttribute("aria-expanded", String(open));
  f.row.classList.toggle("open", open);
}

function refresh() {
  const on = forwards.filter((f) => f.active);
  const any = on.length > 0;
  master.setAttribute("aria-checked", String(any));
  master.disabled = !any && !saved;
  const nActive = lists.true.children.length;
  const nInactive = lists.false.children.length;
  document.getElementById("active-title").hidden = nActive === 0;
  document.getElementById("no-active").hidden = nActive !== 0;
  document.getElementById("inactive-title").hidden = nInactive === 0;
  document.getElementById("inactive-count").textContent = nInactive;
  const state = on.some((f) => f.state === "error") ? "error" : any ? "active" : "idle";
  icon.dataset.state = state;
  icon.setAttribute("aria-label", icon.dataset.quit ? T.iconQuit : state === "error" ? T.iconError : T.icon);
}

// Move a row to the list that matches its switch, keeping the app's order and the keyboard focus.
function place(f) {
  const list = lists[f.active];
  if (f.row.parentNode !== list) {
    const hadFocus = document.activeElement === f.sw;
    const next = [...list.children].find((r) => forwards.find((x) => x.row === r).order > f.order);
    list.insertBefore(f.row, next || null);
    if (hadFocus) {
      f.sw.focus({ preventScroll: true });
      if (document.activeElement !== f.sw) disc.focus({ preventScroll: true });
    }
  }
  refresh();
}

// The notice bar under the panel, like the app's.
const noticeEl = document.getElementById("notice");
let noticeTimer = 0;
function notice(text) {
  document.getElementById("notice-text").textContent = text;
  noticeEl.hidden = false;
  clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => { noticeEl.hidden = true; }, 5000);
}
document.getElementById("notice-x").addEventListener("click", () => { noticeEl.hidden = true; });

function settle(f) {
  if (!f.active) return;
  f.state = f.fails ? "error" : "connected";
  paint(f);
  refresh();
  say(f.fails ? fmt(T.sayError, f.id, T.reasonBilling) : fmt(T.sayConnected, f.id));
}

function setActive(f, on, quiet) {
  if (f.active === on) return;
  clearTimeout(f.settleTimer);
  clearTimeout(f.moveTimer);
  f.active = on;
  f.state = on ? "reconnecting" : null;
  if (!on) toggle(f, false);
  paint(f);
  if (on) {
    // Competing Forwards share a local port: at most one is Active.
    forwards.filter((o) => o !== f && o.active && o.port === f.port).forEach((o) => {
      setActive(o, false, true);
      notice(fmt(T.noticeRival, o.id, f.id));
    });
    if (reduced) settle(f);
    else f.settleTimer = setTimeout(() => settle(f), 700);
  } else if (!quiet) {
    say(fmt(T.sayOff, f.id));
  }
  if (reduced) place(f);
  else f.moveTimer = setTimeout(() => place(f), 220);
  refresh();
}

panel.addEventListener("click", (e) => {
  const mini = e.target.closest(".mini");
  if (mini) {
    const f = forwards.find((x) => x.row === mini.closest(".row"));
    navigator.clipboard?.writeText("localhost:" + f.port).catch(() => {});
    say(T.sayCopied);
    return;
  }
  const sw = e.target.closest(".sw");
  if (sw) {
    if (sw.disabled || sw.id === "set-login") return;
    if (sw === master) {
      if (master.getAttribute("aria-checked") === "true") {
        saved = forwards.filter((f) => f.active);
        saved.forEach((f) => setActive(f, false, true));
        say(T.sayAllOff);
      } else {
        (saved || []).forEach((f) => setActive(f, true));
        saved = null;
        say(T.sayAllOn);
      }
      refresh();
      return;
    }
    const f = forwards.find((x) => x.sw === sw);
    setActive(f, !f.active);
    return;
  }
  const line = e.target.closest(".line");
  if (line) toggle(forwards.find((x) => x.row === line.closest(".row")));
});

disc.addEventListener("click", () => {
  const open = disc.getAttribute("aria-expanded") !== "true";
  disc.setAttribute("aria-expanded", String(open));
  inactiveBox.hidden = !open;
});

function setOpen(open) {
  wrap.classList.toggle("closed", !open);
  icon.setAttribute("aria-expanded", String(open));
  if (!open && panel.contains(document.activeElement)) icon.focus();
}
icon.addEventListener("click", () => {
  if (icon.dataset.quit) { // Quit Kubeport, then open it again from the menu bar.
    delete icon.dataset.quit;
    refresh();
    setOpen(true);
    return;
  }
  setOpen(wrap.classList.contains("closed"));
});

// --- Screens: the list, the form and the settings slide like the app's navigation. ---
const screens = document.getElementById("screens");
const screenEls = {
  list: document.getElementById("screen-list"),
  form: document.getElementById("screen-form"),
  settings: document.getElementById("screen-settings"),
};
const newButton = document.getElementById("new-forward");
const settingsButton = document.getElementById("open-settings");
const formEl = document.getElementById("nf");
const fName = document.getElementById("nf-name");
const fLocal = document.getElementById("nf-local");
const fSave = document.getElementById("nf-save");
const fError = document.getElementById("nf-error");
let opener = newButton;
let sliding = 0;

function go(to) {
  if (sliding) return;
  const showing = screenEls[to];
  const leaving = Object.values(screenEls).find((el) => !el.hidden);
  if (leaving === showing) return;
  const focusTarget = to === "form" ? document.getElementById("nf-context")
    : to === "settings" ? document.getElementById("set-login") : opener;
  if (to === "form") resetForm();
  if (reduced) {
    leaving.hidden = true;
    showing.hidden = false;
    focusTarget.focus({ preventScroll: true });
    return;
  }
  const from = screens.offsetHeight;
  screens.style.height = from + "px";
  screens.classList.add("sliding");
  showing.hidden = false;
  const dir = to === "list" ? -1 : 1;
  showing.style.transform = `translateX(${dir * 100}%)`;
  leaving.style.transform = "translateX(0)";
  const target = showing.offsetHeight;
  void screens.offsetWidth; // commit the start positions
  screens.style.height = target + "px";
  showing.style.transform = "translateX(0)";
  leaving.style.transform = `translateX(${-dir * 100}%)`;
  focusTarget.focus({ preventScroll: true });
  sliding = setTimeout(() => {
    leaving.hidden = true;
    leaving.style.transform = showing.style.transform = "";
    screens.classList.remove("sliding");
    screens.style.height = "";
    sliding = 0;
  }, 260);
}
function showList() { // no slide: after Quit
  clearTimeout(sliding);
  sliding = 0;
  screens.classList.remove("sliding");
  screens.style.height = "";
  Object.entries(screenEls).forEach(([name, el]) => { el.hidden = name !== "list"; el.style.transform = ""; });
}

function resetForm() {
  fName.value = "checkout";
  fLocal.value = "8080";
  formEl.querySelector('input[value="service"]').checked = true;
  syncKind();
  document.getElementById("nf-idle").value = "inherit";
  fError.textContent = "";
  validate();
}
function validate() {
  const port = Number(fLocal.value);
  const ok = fName.value.trim() !== "" && Number.isInteger(port) && port > 0 && port < 65536;
  fSave.disabled = !ok;
  return ok;
}
// Service or Pod: the target menu follows the choice.
function syncKind() {
  const pod = formEl.querySelector('input[name="kind"]:checked').value === "pod";
  document.getElementById("nf-target-label").textContent = pod ? T.pod : T.service;
  document.getElementById("nf-service-w").hidden = pod;
  document.getElementById("nf-pod-w").hidden = !pod;
  document.getElementById("nf-target").htmlFor = pod ? "nf-pod" : "nf-service";
}
formEl.addEventListener("change", (e) => { if (e.target.name === "kind") syncKind(); });
formEl.addEventListener("input", () => { fError.textContent = ""; validate(); });
formEl.addEventListener("submit", (e) => {
  e.preventDefault();
  if (!validate()) return;
  const name = fName.value.trim();
  if (forwards.some((f) => f.id === name)) {
    fError.textContent = fmt(T.dupName, name);
    return;
  }
  addForward(name, String(Number(fLocal.value)));
  go("list");
});
document.getElementById("nf-back").addEventListener("click", () => go("list"));
document.getElementById("nf-cancel").addEventListener("click", () => go("list"));
document.getElementById("set-back").addEventListener("click", () => go("list"));
newButton.addEventListener("click", () => { opener = newButton; go("form"); });
settingsButton.addEventListener("click", () => { opener = settingsButton; go("settings"); });

// Save: the new Forward is Active, like in the app, and goes through Reconnecting to Connected.
function addForward(name, port) {
  const row = template.cloneNode(true);
  const n = ++idSeq;
  row.dataset.id = name;
  row.dataset.active = "true";
  row.classList.remove("open");
  row.removeAttribute("data-fails");
  row.querySelector(".n").textContent = name;
  row.querySelector(".port").textContent = ":" + port;
  const det = row.querySelector(".details");
  det.id = "d-new" + n;
  det.hidden = true;
  const btn = row.querySelector(".rbtn");
  btn.setAttribute("aria-controls", det.id);
  btn.setAttribute("aria-expanded", "false");
  row.querySelector(".d-target").textContent = "svc/checkout:8080";
  row.querySelector(".d-pod-v").textContent = "checkout-6f7c9d8b5-x2k4q";
  row.querySelector(".d-addr").textContent = "localhost:" + port;
  row.querySelector(".sw").setAttribute("aria-label", fmt(T.rowSwitch, name));
  const f = track(row);
  f.idleSetting = document.getElementById("nf-idle").value;
  f.active = false; // setActive turns it on and runs the Reconnecting stage
  f.state = null;
  lists.true.append(row);
  f.sw.setAttribute("aria-checked", "false");
  setActive(f, true, true);
  place(f);
  say(fmt(T.sayAdded, name));
}

// --- Settings: the switch and the menu work; nothing touches a file. ---
const loginSwitch = document.getElementById("set-login");
loginSwitch.addEventListener("click", () => {
  loginSwitch.setAttribute("aria-checked", String(loginSwitch.getAttribute("aria-checked") !== "true"));
});
const idleSelect = document.getElementById("set-idle");
const inheritOption = document.querySelector('#nf-idle option[value="inherit"]');
const syncInherit = () => { inheritOption.textContent = fmt(T.inherit, idleSelect.selectedOptions[0].textContent); };
idleSelect.addEventListener("change", () => { syncInherit(); forwards.forEach(paint); });
syncInherit();
document.getElementById("set-apply").addEventListener("click", () => notice(T.noticeApplied));
["set-choose", "set-config", "set-reload"].forEach((id) =>
  document.getElementById(id).addEventListener("click", () => notice(T.noticeDemo)));
document.getElementById("set-quit").addEventListener("click", () => {
  icon.dataset.quit = "1";
  setOpen(false);
  showList();
  refresh();
  icon.focus();
});

panel.addEventListener("keydown", (e) => {
  if (e.key !== "Escape") return;
  if (screenEls.list.hidden) go("list");
  else { setOpen(false); icon.focus(); }
});

forwards.forEach(paint);
refresh();

// --- The hero demo: until the visitor touches the panel, it plays a rollout on
// orders-api and the idle timeout on grafana, then starts over. ---
const hintSub = document.getElementById("hint-sub");
const orders = byId("orders-api");
const grafana = byId("grafana");
const NEW_POD = "orders-api-84b7c5d9f-q7v3x";
let demoLive = false;
let demoLoop = null;

function demoReset() {
  forwards.forEach((f) => toggle(f, false));
  orders.demoReason = null;
  orders.state = "connected";
  orders.podName = orders.pod0;
  grafana.demoIdle = null;
  noticeEl.hidden = true;
  forwards.forEach(paint);
  refresh();
}
const demoIdle = (minutes) => () => { grafana.demoIdle = minutes; paint(grafana); };
const DEMO_EVENTS = [
  [1500, () => toggle(orders, true)],
  [3200, () => { orders.state = "reconnecting"; orders.demoReason = T.reasonNoPod; paint(orders); refresh(); }],
  [5000, () => { orders.state = "connected"; orders.demoReason = null; orders.podName = NEW_POD; paint(orders); refresh(); }],
  [7000, () => { toggle(orders, false); toggle(grafana, true); demoIdle(60)(); }],
  [7700, demoIdle(45)],
  [8400, demoIdle(30)],
  [9100, demoIdle(15)],
  [9800, demoIdle(5)],
  [10500, demoIdle(1)],
  [11300, () => { grafana.demoIdle = null; setActive(grafana, false, true); notice(fmt(T.noticeIdle, "grafana", dur(60))); }],
  [14000, () => { noticeEl.hidden = true; setActive(grafana, true, true); }],
];

// The visitor takes over: the demo stops for good and leaves a clean panel.
function takeOver() {
  if (!demoLive) return;
  demoLive = false;
  demoLoop.stop();
  silent = false;
  orders.demoReason = null;
  orders.state = "connected";
  orders.podName = NEW_POD;
  grafana.demoIdle = null;
  if (!grafana.active) setActive(grafana, true, true);
  forwards.forEach((f) => toggle(f, false));
  noticeEl.hidden = true;
  forwards.forEach(paint);
  refresh();
  forwards[0].sw.classList.remove("nudge");
  hintSub.textContent = T.hintMine;
}
["pointerdown", "keydown", "focusin"].forEach((type) => {
  panel.addEventListener(type, takeOver, true);
  icon.addEventListener(type, takeOver, true);
});

if (!reduced) {
  demoLive = true;
  silent = true;
  hintSub.textContent = T.hintDemo;
  demoLoop = makeLoop({ events: DEMO_EVENTS, period: 15500, onStart: demoReset });
  demoLoop.observe(wrap, 0.4);
}

// "Try it in the panel": scroll to the hero, open the panel and show the form.
document.getElementById("try-form")?.addEventListener("click", () => {
  takeOver();
  scrollTo({ top: 0, behavior: reduced ? "auto" : "smooth" });
  delete icon.dataset.quit;
  opener = newButton;
  setOpen(true);
  go("form");
  refresh();
});

// The memory bars grow in once when they scroll into view.
const bars = document.getElementById("bars");
if (!reduced) {
  bars.classList.add("pre");
  new IntersectionObserver((entries, observer) => {
    if (entries.some((e) => e.isIntersecting)) {
      bars.classList.remove("pre");
      bars.classList.add("in");
      observer.disconnect();
    }
  }, { threshold: 0.6 }).observe(bars);
}

// ---------------------------------------------------------------------------
// The rollout: a loop. The markup ships with the end state, so it reads without
// JavaScript and with reduced motion.
// ---------------------------------------------------------------------------
const lanesEl = document.getElementById("lanes");
const lanes = [...document.querySelectorAll(".lane[data-lane]")].map((lane) => {
  const el = (name) => lane.querySelector(`[data-el="${name}"]`);
  return { kind: lane.dataset.lane, el };
});
const frame = (la, lb, pa, pb, ta, tb, status) => ({ la, lb, pa, pb, ta, tb, status });
const START = frame("link up", "link up hidden", "pod ready", "pod starting hidden", "label", "label hidden", T.rStart);
// What kubectl prints is the same in every language.
const K_LINE = '<span class="tl">Forwarding from 127.0.0.1:5432 -&gt; 5432</span>';
const K_ERR = "error: lost connection to pod";
const K_END = `${K_LINE}<span class="tl err">${K_ERR}</span><span class="sentence">${T.rAgain}</span>`;
const FRAMES = {
  kubectl: [
    { ...START, status: K_LINE },
    frame("link up", "link up hidden", "pod dying", "pod starting", "label", "label",
      `${K_LINE}<span class="sentence">${T.rRollout}</span>`),
    frame("link down", "link up hidden", "pod dying", "pod starting", "label", "label", "typed"),
    frame("link down", "link up hidden", "pod dying", "pod ready", "label", "label", K_END),
    frame("link down hidden", "link up hidden", "pod dying gone", "pod ready", "label hidden", "label", K_END),
  ],
  kubeport: [
    START,
    frame("link up", "link up hidden", "pod dying", "pod starting", "label", "label", T.rRollout),
    frame("link wait", "link up hidden", "pod dying", "pod starting", "label", "label", T.rWait),
    frame("link hidden", "link up draw", "pod dying", "pod ready", "label", "label", T.rOk),
    frame("link hidden", "link up draw", "pod dying gone", "pod ready", "label hidden", "label", T.rOk),
  ],
};

// The kubectl error is typed like a terminal. Pausing completes it at once.
let typing = null;
function stopTyping(complete) {
  if (!typing) return;
  clearTimeout(typing.timer);
  if (complete) typing.finish();
  typing = null;
}
function typeError(status) {
  const done = () => { status.innerHTML = K_END; };
  status.innerHTML = `${K_LINE}<span class="tl err"><span class="t"></span><i class="caret"></i></span>`;
  const target = status.querySelector(".t");
  let n = 0;
  typing = { finish: done, timer: 0 };
  const tick = () => {
    n += 1;
    target.textContent = K_ERR.slice(0, n);
    if (n < K_ERR.length) typing.timer = setTimeout(tick, 28);
    else { typing = null; done(); }
  };
  typing.timer = setTimeout(tick, 200);
}

const running = (cls) => /\bup\b/.test(cls) && !/\bhidden\b/.test(cls);
function render(index) {
  stopTyping(false);
  for (const { kind, el } of lanes) {
    const f = FRAMES[kind][index];
    el("link-a").setAttribute("class", f.la);
    el("link-b").setAttribute("class", f.lb);
    el("pod-a").setAttribute("class", f.pa);
    el("pod-b").setAttribute("class", f.pb);
    el("label-a").setAttribute("class", f.ta);
    el("label-b").setAttribute("class", f.tb);
    el("pk-a").setAttribute("class", running(f.la) ? "packets run" : "packets");
    el("pk-b").setAttribute("class", running(f.lb) ? "packets run late" : "packets");
    if (f.status === "typed") typeError(el("status"));
    else el("status").innerHTML = f.status;
  }
}

// ---------------------------------------------------------------------------
// The idle timeout: the same drawing with a clock. No connection: the clock runs down.
// A connection resets it. At zero the tunnel closes and the port is free.
// ---------------------------------------------------------------------------
const idleCard = document.getElementById("idle-card");
const idleEl = (name) => idleCard.querySelector(`[data-el="${name}"]`);
const IDLE = [ // [ms, minutes left, connection open, status]
  [0, 60, false, "iNoConn"], [600, 45, false, "iNoConn"], [1200, 30, false, "iNoConn"],
  [1800, 60, true, "iConn"], [3200, 60, false, "iNoConn2"],
  [3800, 45, false, "iNoConn2"], [4400, 30, false, "iNoConn2"], [5000, 15, false, "iNoConn2"],
  [5600, 5, false, "iNoConn2"], [6200, 1, false, "iNoConn2"],
  [6800, 0, false, "iClosed"],
];
function renderIdle(i) {
  const [, minutes, connection, key] = IDLE[i];
  const closed = minutes === 0;
  idleEl("clock").textContent = closed ? T.stateInactive : fmt(T.idleCountdown, dur(minutes));
  const bar = idleEl("bar");
  bar.style.setProperty("--r", minutes / 60);
  bar.classList.toggle("low", minutes > 0 && minutes <= 5);
  idleEl("link").setAttribute("class", closed ? "link hidden" : "link up");
  idleEl("pk").setAttribute("class", connection ? "packets run" : "packets");
  idleEl("port").textContent = closed ? T.portFree : T.portUse;
  idleEl("status").innerHTML = T[key];
}

if (!reduced) {
  render(0);
  renderIdle(0);
  const rollout = makeLoop({
    events: [[1200, 1], [2600, 2], [4000, 3], [5000, 4]].map(([t, i]) => [t, () => render(i)]),
    period: 9000,
    onStart: () => render(0),
    onPause: () => stopTyping(true),
    container: lanesEl,
  });
  rollout.observe(lanesEl, 0.25);
  wirePause(document.getElementById("motion"), rollout);
  const idle = makeLoop({
    events: IDLE.slice(1).map(([t], k) => [t, () => renderIdle(k + 1)]),
    period: 10800,
    onStart: () => renderIdle(0),
    container: idleCard,
  });
  idle.observe(idleCard, 0.4);
  wirePause(document.getElementById("motion-idle"), idle);
} else {
  document.getElementById("motion").hidden = true;
  document.getElementById("motion-idle").hidden = true;
}
