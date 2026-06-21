const { invoke } = window.__TAURI__.core;
const opener = window.__TAURI__.opener;
const event = window.__TAURI__.event;
const updater = window.__TAURI__.updater;
const process_api = window.__TAURI__.process;

const MOD_KEYS = ["old_models", "classic_spells", "takp_icons"];

const els = {};
let toastTimer = null;
let statusTimer = null;
let PLATFORM = "macos"; // "windows" | "macos" | "linux"; set on boot
const isWindows = () => PLATFORM === "windows";

function $(id) {
  return document.getElementById(id);
}

function showToast(message) {
  els.toast.textContent = message;
  els.toast.classList.add("show");
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => els.toast.classList.remove("show"), 2400);
}

function formatUptime(seconds) {
  if (!seconds || seconds < 0) return "—";
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${mins}m`;
  return `${mins}m`;
}

function formatTime(unix) {
  if (!unix) return "—";
  const d = new Date(unix * 1000);
  return d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

async function refreshStatus() {
  els.statusDot.dataset.state = "checking";
  els.statusLabel.textContent = "Checking…";
  try {
    const status = await invoke("get_server_status");
    els.statusDot.dataset.state = status.online ? "online" : "offline";
    els.statusLabel.textContent = status.online ? "Online" : "Offline";
    els.statName.textContent = status.server_name || "—";
    els.statHost.textContent = status.host || "—";
    if (status.online) {
      els.statPlayers.textContent = status.players > 0 ? String(status.players) : "—";
      els.statUptime.textContent =
        status.uptime_seconds > 0 ? formatUptime(status.uptime_seconds) : "—";
    } else {
      els.statPlayers.textContent = "—";
      els.statUptime.textContent = "—";
    }
    els.statChecked.textContent = formatTime(status.last_check_unix);
  } catch (err) {
    els.statusDot.dataset.state = "offline";
    els.statusLabel.textContent = "Unreachable";
    els.statChecked.textContent = formatTime(Math.floor(Date.now() / 1000));
    console.error("status fetch failed", err);
  }
}

async function loadMods() {
  try {
    const state = await invoke("get_mod_state");
    els.modOldModels.checked = !!state.old_models;
    els.modClassicSpells.checked = !!state.classic_spells;
    els.modTakpIcons.checked = !!state.takp_icons;
    els.perfPreset.value = state.perf_preset || "medium";
  } catch (err) {
    console.error("load mods failed", err);
    showToast("Could not read mod state");
  }
}

async function saveMods() {
  const state = {
    old_models: els.modOldModels.checked,
    classic_spells: els.modClassicSpells.checked,
    takp_icons: els.modTakpIcons.checked,
    perf_preset: els.perfPreset.value,
  };
  try {
    await invoke("set_mod_state", { state });
    showToast("Mods saved");
  } catch (err) {
    console.error("save mods failed", err);
    showToast(`Save failed: ${err}`);
  }
}

async function loadPatchNotes() {
  try {
    const entries = await invoke("get_patch_notes");
    if (!entries.length) {
      els.notesScroll.innerHTML = '<p class="placeholder">No patch notes available.</p>';
      return;
    }
    els.notesScroll.innerHTML = entries.map(renderEntry).join("");
    els.notesSource.textContent = `${entries.length} most recent`;
  } catch (err) {
    els.notesScroll.innerHTML = '<p class="placeholder">Patch notes unavailable.</p>';
    console.error("patch notes failed", err);
  }
}

function escapeHtml(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function renderInline(s) {
  return escapeHtml(s)
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*]+)\*/g, "<em>$1</em>");
}

function renderMarkdown(body) {
  const lines = body.split("\n");
  const out = [];
  let listOpen = false;
  for (const raw of lines) {
    const line = raw.trimEnd();
    if (!line) {
      if (listOpen) { out.push("</ul>"); listOpen = false; }
      out.push("");
      continue;
    }
    let m;
    if ((m = line.match(/^###\s+(.*)$/))) {
      if (listOpen) { out.push("</ul>"); listOpen = false; }
      out.push(`<h4>${renderInline(m[1])}</h4>`);
    } else if ((m = line.match(/^##\s+(.*)$/))) {
      if (listOpen) { out.push("</ul>"); listOpen = false; }
      out.push(`<h4>${renderInline(m[1])}</h4>`);
    } else if ((m = line.match(/^[-*]\s+(.*)$/))) {
      if (!listOpen) { out.push("<ul>"); listOpen = true; }
      out.push(`<li>${renderInline(m[1])}</li>`);
    } else {
      if (listOpen) { out.push("</ul>"); listOpen = false; }
      out.push(`<p>${renderInline(line)}</p>`);
    }
  }
  if (listOpen) out.push("</ul>");
  return out.join("\n");
}

function renderEntry(entry) {
  return `
    <article class="note-entry">
      <span class="date">${escapeHtml(entry.date)}</span>
      <h3>${escapeHtml(entry.title || "Patch")}</h3>
      <div class="body">${renderMarkdown(entry.body)}</div>
    </article>
  `;
}

function formatBytes(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

let _patchUnlisten = null;

async function runPatcher() {
  els.launchMeta.textContent = "Checking for client updates…";
  let check;
  try {
    check = await invoke("check_for_updates");
  } catch (err) {
    console.error("update check failed", err);
    els.launchMeta.textContent = "Update check failed — launching anyway.";
    showToast(`Update check failed: ${err}`);
    return;
  }

  if (check.files_to_update.length === 0) {
    els.launchMeta.textContent = `Client up to date (manifest ${check.manifest_version}).`;
    return;
  }

  els.launchMeta.textContent = `${check.files_to_update.length} file(s) to update — ${formatBytes(check.total_bytes)}…`;

  if (_patchUnlisten) _patchUnlisten();
  _patchUnlisten = await event.listen("patch-progress", (e) => {
    const p = e.payload;
    if (p.stage === "downloading") {
      const pct = p.bytes_total
        ? Math.floor((p.bytes_done * 100) / p.bytes_total)
        : 0;
      els.launchMeta.textContent =
        `Patching ${p.files_done}/${p.files_total} · ${formatBytes(p.bytes_done)}/${formatBytes(p.bytes_total)} (${pct}%) — ${p.current_file}`;
    } else if (p.stage === "done") {
      els.launchMeta.textContent = `Patch complete — ${p.files_done} file(s) updated.`;
    }
  });

  try {
    const updated = await invoke("apply_updates");
    els.launchMeta.textContent = `Patched ${updated} file(s). Opening Norrath…`;
  } catch (err) {
    console.error("apply updates failed", err);
    els.launchMeta.textContent = "Patcher failed — launching anyway.";
    showToast(`Patcher failed: ${err}`);
  } finally {
    if (_patchUnlisten) {
      _patchUnlisten();
      _patchUnlisten = null;
    }
  }
}

async function launch() {
  els.enterWorld.disabled = true;
  try {
    await runPatcher();
    els.launchMeta.textContent = "Opening Norrath…";
    await invoke("launch_eq");
    showToast("Launching EverQuest");
    setTimeout(() => {
      els.enterWorld.disabled = false;
      els.launchMeta.textContent = "May your bind point be safe.";
    }, 4000);
  } catch (err) {
    els.enterWorld.disabled = false;
    els.launchMeta.textContent = "Launch failed.";
    showToast(`Launch failed: ${err}`);
    console.error("launch failed", err);
  }
}

function renderPreflight(report) {
  els.preflightStrip.innerHTML = report.checks
    .map(
      (c) => `
        <li class="pf ${c.ok ? "ok" : "bad"}" title="${escapeHtml(c.detail)}">
          <span class="pf-dot"></span>
          <span class="pf-label">${escapeHtml(c.label)}</span>
        </li>`,
    )
    .join("");
  els.enterWorld.disabled = !report.ready;
  if (!report.ready) {
    els.launchMeta.textContent = "Setup needed before you can launch.";
  }
}

async function loadPreflight() {
  try {
    const report = await invoke("get_preflight");
    renderPreflight(report);
    return report;
  } catch (err) {
    console.error("preflight failed", err);
    return null;
  }
}

// ---------------------------------------------------------------------------
// First-run wizard — interactive Whisky / prefix / client / eqhost setup.
// ---------------------------------------------------------------------------

const WIZARD_STEPS = [
  {
    key: "whisky",
    label: "Install Whisky",
    detailDone: "Whisky.app found at /Applications/Whisky.app",
    detailPending:
      "Whisky is a free, open-source Wine wrapper for Mac. Click below — it opens the download page.",
    actionLabel: "Download Whisky",
    handler: openWhiskyDownload,
    autoHandler: null,
    blockingDep: null,
  },
  {
    key: "wine_prefix",
    label: "Initialize Wine prefix",
    detailDone: "Wine prefix ready",
    detailPending:
      "Creates a fresh Wine environment for EverQuest under Application Support. Takes ~10 seconds.",
    actionLabel: "Set up",
    handler: () => runSetupCommand("setup_wine_prefix", "wine_prefix", "Wine prefix initialized"),
    blockingDep: "whisky_installed",
  },
  {
    key: "client",
    label: "Locate your EverQuest client",
    detailDone: null, // filled at render-time with eq_dir
    detailPending:
      "The Last Camp doesn't distribute the EverQuest client. Point us at your RoF2 folder (the one with eqgame.exe). Don't have it yet? Ask in our Discord.",
    actionLabel: null,
    handler: null,
    blockingDep: "prefix_ready",
  },
  {
    key: "eqhost",
    label: "Set login server",
    detailDone: "Pointing at play.thelastcamp.net:5999",
    detailPending: "Writes eqhost.txt so the client connects to The Last Camp.",
    actionLabel: "Set",
    handler: () => runSetupCommand("setup_eqhost", "eqhost", "Login server set"),
    blockingDep: "client_installed",
  },
];

const STEP_DEPS = {
  wine_prefix: "whisky_installed",
  client: "prefix_ready",
  eqhost: "client_installed",
};

function isStepDone(state, stepKey) {
  return {
    whisky: state.whisky_installed,
    wine_prefix: state.prefix_ready,
    client: state.client_installed,
    eqhost: state.eqhost_set,
  }[stepKey];
}

function isStepBlocked(state, stepKey) {
  const dep = STEP_DEPS[stepKey];
  return dep ? !state[dep] : false;
}

function renderWizardSteps(state) {
  // Windows runs eqgame.exe natively — no Whisky / Wine prefix steps.
  const steps = WIZARD_STEPS.filter(
    (s) => !(isWindows() && (s.key === "whisky" || s.key === "wine_prefix")),
  );

  els.setupSteps.innerHTML = steps.map((step, idx) => {
    const done = isStepDone(state, step.key);
    const blocked = isStepBlocked(state, step.key);
    const cls = done ? "done" : blocked ? "blocked" : "pending";
    let detail;
    if (done) {
      detail = step.key === "client"
        ? `Installed at ${escapeHtml(state.eq_dir)}`
        : escapeHtml(step.detailDone);
    } else if (step.key === "client") {
      detail =
        "Point us at your RoF2 client folder (the one with eqgame.exe). Don't have it? Get it from our Discord.";
    } else {
      detail = escapeHtml(step.detailPending);
    }
    // Bring-your-own-client: let players point at an existing client folder
    // on both Mac and Windows.
    const folderPicker =
      !done && step.key === "client"
        ? `<div class="folder-pick">
             <input type="text" class="folder-input" id="eqdir-input"
                    placeholder="Path to your RoF2 folder (contains eqgame.exe)" value="${escapeHtml(state.eq_dir || "")}" />
             <button class="ghost" id="eqdir-use">Use folder</button>
           </div>`
        : "";
    let trailing;
    if (done) {
      trailing = `<span class="step-check" aria-label="Done">✓</span>`;
    } else if (step.actionLabel) {
      trailing = `<button class="ghost step-action" data-step="${step.key}" ${blocked ? "disabled" : ""}>${escapeHtml(step.actionLabel)}</button>`;
    } else {
      // No download action (e.g. bring-your-own-client step) — the folder
      // picker's "Use folder" button is the only control.
      trailing = "";
    }
    return `
      <li class="setup-step ${cls}" data-step="${step.key}">
        <span class="step-num">${idx + 1}</span>
        <div class="step-body">
          <strong>${escapeHtml(step.label)}</strong>
          <em>${detail}</em>
          ${folderPicker}
          <div class="step-progress" data-progress="${step.key}" hidden>
            <div class="step-bar"><div class="step-bar-fill" style="width:0%"></div></div>
            <span class="step-progress-label"></span>
          </div>
        </div>
        ${trailing}
      </li>`;
  }).join("");

  els.setupSteps.querySelectorAll(".step-action").forEach((btn) => {
    const stepKey = btn.dataset.step;
    const step = WIZARD_STEPS.find((s) => s.key === stepKey);
    if (step?.handler) btn.addEventListener("click", step.handler);
  });

  const useBtn = els.setupSteps.querySelector("#eqdir-use");
  if (useBtn) useBtn.addEventListener("click", useExistingEqDir);

  els.setupLaunch.disabled = !state.ready;
}

async function useExistingEqDir() {
  const input = els.setupSteps.querySelector("#eqdir-input");
  const path = (input?.value || "").trim();
  if (!path) {
    showToast("Enter the folder that contains eqgame.exe.");
    return;
  }
  try {
    await invoke("set_eq_dir", { path });
    showToast("Game folder set. Re-checking…");
    await loadSetupState();
  } catch (err) {
    showToast(`Couldn't use that folder: ${err}`);
  }
}

async function loadSetupState({ openWizardIfNeeded = false } = {}) {
  try {
    const state = await invoke("get_setup_state");
    renderWizardSteps(state);
    if (state.ready) {
      els.setupModal.hidden = true;
      els.enterWorld.disabled = false;
      els.launchMeta.textContent = "Ready when you are, traveler.";
    } else if (openWizardIfNeeded) {
      els.setupModal.hidden = false;
      els.enterWorld.disabled = true;
    }
    return state;
  } catch (err) {
    console.error("setup state failed", err);
    return null;
  }
}

async function openWhiskyDownload() {
  try {
    await invoke("open_whisky_download");
    showToast("Whisky download opened. Drag Whisky.app to /Applications, then hit Re-check.");
  } catch (err) {
    showToast(`Could not open browser: ${err}`);
  }
}

function setStepActive(stepKey, active) {
  const step = els.setupSteps.querySelector(`[data-step="${stepKey}"]`);
  if (!step) return;
  step.classList.toggle("active", active);
  const action = step.querySelector(".step-action");
  if (action) action.disabled = active;
  const progress = step.querySelector(`[data-progress="${stepKey}"]`);
  if (progress) {
    if (active) {
      progress.hidden = false;
      const fill = progress.querySelector(".step-bar-fill");
      if (fill) fill.style.width = "0%";
      const label = progress.querySelector(".step-progress-label");
      if (label) label.textContent = "Starting…";
    }
  }
}

async function runSetupCommand(command, stepKey, successMsg) {
  setStepActive(stepKey, true);
  try {
    await invoke(command);
    showToast(successMsg);
  } catch (err) {
    showToast(`Setup failed: ${err}`);
    console.error(`${command} failed`, err);
  } finally {
    setStepActive(stepKey, false);
    await loadSetupState();
  }
}

let _setupUnlisten = null;
async function subscribeSetupProgress() {
  if (!event?.listen) return;
  if (_setupUnlisten) return;
  _setupUnlisten = await event.listen("setup-progress", (e) => {
    const p = e.payload;
    const wrap = els.setupSteps.querySelector(`[data-progress="${p.step}"]`);
    if (!wrap) return;
    wrap.hidden = false;
    const fill = wrap.querySelector(".step-bar-fill");
    const label = wrap.querySelector(".step-progress-label");
    if (fill) fill.style.width = `${p.percent}%`;
    let stageLabel;
    switch (p.stage) {
      case "downloading": stageLabel = "Downloading"; break;
      case "extracting":  stageLabel = "Extracting"; break;
      case "initializing": stageLabel = "Initializing"; break;
      case "done": stageLabel = "Done"; break;
      case "error": stageLabel = "Error"; break;
      default: stageLabel = p.stage;
    }
    if (label) {
      label.textContent = p.detail
        ? `${stageLabel} · ${p.percent}% · ${p.detail}`
        : `${stageLabel} · ${p.percent}%`;
    }
  });
}

async function runVerify() {
  els.verifyInstall.disabled = true;
  showToast("Verifying client files…");
  try {
    const report = await invoke("verify_install");
    if (report.ok) {
      showToast(`Install OK · ${report.entries.length} files verified`);
    } else {
      const bad = report.entries.filter((e) => !e.ok);
      showToast(`Issues: ${bad.map((b) => b.name).join(", ")}`);
    }
  } catch (err) {
    showToast(`Verify failed: ${err}`);
  } finally {
    els.verifyInstall.disabled = false;
  }
}

async function loadLastCharacter() {
  try {
    const last = await invoke("get_last_character");
    if (!last) return;
    const when = new Date(last.last_played_unix * 1000);
    const ago = formatAgo(Date.now() / 1000 - last.last_played_unix);
    els.launchMeta.innerHTML = `Welcome back, <strong>${escapeHtml(
      last.name,
    )}</strong> · last seen ${ago}`;
    els.launchMeta.title = when.toLocaleString();
  } catch (err) {
    console.error("last character failed", err);
  }
}

function formatAgo(seconds) {
  if (seconds < 90) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  const days = Math.floor(seconds / 86400);
  return days === 1 ? "yesterday" : `${days}d ago`;
}

async function loadModPacks() {
  try {
    const packs = await invoke("get_mod_packs");
    const addonList = $("addon-list");
    addonList.innerHTML = "";
    for (const pack of packs) {
      const li = document.querySelector(`li[data-pack="${pack.key}"]`);
      if (li) {
        const toggleInput = li.querySelector("input[type=checkbox]");
        const action = li.querySelector(".mod-action");
        if (pack.installed) {
          if (toggleInput) toggleInput.disabled = false;
          if (action) action.hidden = true;
        } else {
          if (toggleInput) {
            toggleInput.disabled = true;
            toggleInput.checked = false;
          }
          if (action) {
            action.hidden = false;
            action.disabled = !pack.downloadable;
            action.textContent = pack.downloadable ? "Download" : "Pack pending";
            action.title = pack.downloadable
              ? "Fetch this pack to your _mods folder"
              : "No download URL configured yet";
          }
        }
        continue;
      }
      // Pack not in static toggle UI — render as add-on
      const addon = document.createElement("li");
      addon.className = "addon-row";
      addon.dataset.pack = pack.key;
      const status = pack.installed ? "Installed" : pack.downloadable ? "Download" : "Pending";
      addon.innerHTML = `
        <div class="addon-meta">
          <strong>${escapeHtml(pack.label)}</strong>
          <em>${pack.installed ? `${fmtBytes(pack.bytes)} on disk` : "Optional add-on"}</em>
        </div>
        <button class="ghost mod-action" data-pack="${escapeHtml(pack.key)}"
          ${pack.installed || !pack.downloadable ? "disabled" : ""}>
          ${status}
        </button>
      `;
      addonList.appendChild(addon);
      const btn = addon.querySelector(".mod-action");
      btn.addEventListener("click", () => downloadPack(pack.key, btn));
    }
  } catch (err) {
    console.error("mod packs failed", err);
  }
}

function fmtBytes(n) {
  if (!n) return "";
  const units = ["B", "KB", "MB", "GB"];
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(n < 10 ? 1 : 0)} ${units[i]}`;
}

async function downloadPack(key, button) {
  button.disabled = true;
  const original = button.textContent;
  button.textContent = "Starting…";
  try {
    await invoke("download_mod_pack", { key });
    showToast(`Installed ${key}`);
    await loadModPacks();
  } catch (err) {
    showToast(`Download failed: ${err}`);
    button.disabled = false;
    button.textContent = original;
  }
}

async function subscribeProgress() {
  if (!event?.listen) return;
  await event.listen("mod-pack-progress", (e) => {
    const p = e.payload;
    const btn = document.querySelector(`.mod-action[data-pack="${p.key}"]`);
    if (!btn) return;
    if (p.stage === "extracting") {
      btn.textContent = "Extracting…";
    } else if (p.stage === "done") {
      btn.textContent = "Installed";
    } else if (p.stage === "downloading") {
      btn.textContent = p.total
        ? `${p.percent}% · ${fmtBytes(p.received)}`
        : `${fmtBytes(p.received)}`;
    } else if (p.stage === "connecting") {
      btn.textContent = "Connecting…";
    }
  });
}

window.addEventListener("DOMContentLoaded", () => {
  els.statusPill = $("status-pill");
  els.statusDot = els.statusPill.querySelector(".dot");
  els.statusLabel = els.statusPill.querySelector(".label");
  els.statName = $("stat-name");
  els.statHost = $("stat-host");
  els.statPlayers = $("stat-players");
  els.statUptime = $("stat-uptime");
  els.statChecked = $("stat-checked");
  els.refreshStatus = $("refresh-status");
  els.verifyInstall = $("verify-install");
  els.modOldModels = $("mod-old-models");
  els.modClassicSpells = $("mod-classic-spells");
  els.modTakpIcons = $("mod-takp-icons");
  els.perfPreset = $("perf-preset");
  els.notesScroll = $("notes-scroll");
  els.notesSource = $("notes-source");
  els.enterWorld = $("enter-world");
  els.launchMeta = $("launch-meta");
  els.toast = $("toast");
  els.preflightStrip = $("preflight-strip");
  els.setupModal = $("setup-modal");
  els.setupSteps = $("setup-steps");
  els.setupDismiss = $("setup-dismiss");
  els.setupRecheck = $("setup-recheck");
  els.setupLaunch = $("setup-launch");

  els.refreshStatus.addEventListener("click", refreshStatus);
  els.verifyInstall.addEventListener("click", runVerify);
  for (const key of MOD_KEYS) {
    const el = els[`mod${key.replace(/(^|_)([a-z])/g, (_, _u, c) => c.toUpperCase())}`];
    if (el) el.addEventListener("change", saveMods);
  }
  els.perfPreset.addEventListener("change", saveMods);
  els.enterWorld.addEventListener("click", launch);

  document.querySelectorAll(".mod-action").forEach((btn) => {
    btn.addEventListener("click", () => downloadPack(btn.dataset.pack, btn));
  });

  document.querySelectorAll("a[data-ext]").forEach((a) => {
    a.addEventListener("click", (e) => {
      e.preventDefault();
      const url = a.getAttribute("href");
      if (opener?.openUrl) {
        opener.openUrl(url).catch((err) => console.error("open url failed", err));
      } else {
        window.open(url, "_blank");
      }
    });
  });

  els.updateBanner = $("update-banner");
  els.updateBannerMeta = $("update-banner-meta");
  els.updateInstall = $("update-install");
  els.updateDismiss = $("update-dismiss");

  els.updateDismiss.addEventListener("click", () => {
    els.updateBanner.hidden = true;
  });

  els.setupDismiss.addEventListener("click", () => (els.setupModal.hidden = true));
  els.setupRecheck.addEventListener("click", () => {
    loadSetupState();
    loadPreflight();
  });
  els.setupLaunch.addEventListener("click", () => {
    els.setupModal.hidden = true;
    launch();
  });

  document.addEventListener("keydown", (e) => {
    const tag = document.activeElement?.tagName;
    const inForm = tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA";
    if (e.key === "Enter" && els.setupModal.hidden && !els.enterWorld.disabled && !inForm) {
      launch();
    }
    if (e.key === "Escape" && !els.setupModal.hidden) {
      els.setupModal.hidden = true;
    }
  });

  refreshStatus();
  loadMods();
  loadModPacks();
  loadPatchNotes();
  subscribeSetupProgress();
  loadLastCharacter();
  subscribeProgress();
  checkLauncherUpdate();

  // Resolve platform first so the wizard renders the right (Windows vs macOS)
  // steps, then load setup/preflight which depend on it.
  (async () => {
    try {
      PLATFORM = await invoke("get_platform");
    } catch (_) {
      PLATFORM = "macos";
    }
    document.body.dataset.platform = PLATFORM;
    loadPreflight();
    loadSetupState({ openWizardIfNeeded: true });
  })();

  statusTimer = setInterval(refreshStatus, 60_000);
});

async function checkLauncherUpdate() {
  if (!updater?.check) return;
  try {
    const update = await updater.check();
    if (!update?.available) return;
    const meta = `v${update.version || "?"} ready (you have v${update.currentVersion || "?"})`;
    els.updateBannerMeta.textContent = meta;
    els.updateBanner.hidden = false;
    els.updateInstall.onclick = async () => {
      els.updateInstall.disabled = true;
      els.updateInstall.textContent = "Downloading…";
      try {
        await update.downloadAndInstall();
        if (process_api?.relaunch) await process_api.relaunch();
      } catch (err) {
        console.error("launcher update failed", err);
        els.updateInstall.disabled = false;
        els.updateInstall.textContent = "Install & restart";
        showToast("Launcher update failed; check console");
      }
    };
  } catch (err) {
    console.error("launcher update check failed", err);
  }
}

window.addEventListener("beforeunload", () => {
  if (statusTimer) clearInterval(statusTimer);
  if (toastTimer) clearTimeout(toastTimer);
});
