const invoke = window.__TAURI__.core.invoke;

const $ = (id) => document.getElementById(id);

const els = {
  play: $("play"),
  playSub: $("playSub"),
  refresh: $("refresh"),
  blurb: $("blurb"),
  channelLine: $("channelLine"),
  notice: $("notice"),
  profile: $("profile"),
  avatar: $("avatar"),
  profileName: $("profileName"),
  profileSub: $("profileSub"),
  mStatus: $("mStatus"),
  mVersion: $("mVersion"),
  mChannel: $("mChannel"),
  mLauncher: $("mLauncher"),
  mServers: $("mServers"),
};

els.signin = $("signin");
els.signinSub = $("signinSub");
els.hostToggle = $("hostToggle");
els.hostSettings = $("hostSettings");
els.hostAuthorize = $("hostAuthorize");
els.hostConnect = $("hostConnect");
els.addrList = $("addrList");
els.hostDetail = $("hostDetail");
els.hostAuth = $("hostAuth");
els.hostCode = $("hostCode");
els.hostUrl = $("hostUrl");
els.hostLog = $("hostLog");
els.mHost = $("mHost");

// The settings dialog. Every id here maps to one field of the Rust `Settings`
// struct; `FIELDS` below is the single place that mapping is written down.
els.modal = $("settingsModal");
els.settingsLive = $("settingsLive");
els.settingsRegen = $("settingsRegen");
els.settingsError = $("settingsError");

let install = null;
let auth = null;
let host = null;
let hostTimer = null;
let settings = null;
let worldExists = false;

function setNotice(message, kind) {
  if (!message) {
    els.notice.hidden = true;
    return;
  }
  els.notice.textContent = message;
  els.notice.className = kind === "ok" ? "notice ok" : "notice";
  els.notice.hidden = false;
}

function set(el, value, cls) {
  el.textContent = value ?? "—";
  el.className = cls ?? "";
}

function render() {
  if (!install) return;

  set(els.mChannel, install.patchline);
  set(els.mLauncher, install.launcherVersion);
  set(
    els.mServers,
    install.serverCount === null || install.serverCount === undefined
      ? "—"
      : String(install.serverCount)
  );

  const version = install.gameVersion
    ? `${install.gameVersion}${install.gameBuild ? ` (build ${install.gameBuild})` : ""}`
    : "—";
  set(els.mVersion, version);

  if (install.profileName) {
    els.profile.classList.add("on");
    els.avatar.textContent = install.profileName.charAt(0).toUpperCase();
    els.profileName.textContent = install.profileName;
    els.profileSub.textContent = "Last signed in";
  } else {
    els.profile.classList.remove("on");
    els.avatar.textContent = "?";
    els.profileName.textContent = "No profile yet";
    els.profileSub.textContent = "Not signed in";
  }

  if (install.found) {
    set(els.mStatus, "Ready", "good");
    els.channelLine.textContent = install.patchline
      ? `${install.patchline} channel`
      : " ";
    const signedIn = auth && auth.signedIn;
    const canSignIn = auth && auth.configured;

    // Only offer the Sign in button when it can actually succeed, so a missing
    // client_id never reads as a failure.
    els.play.hidden = false;
    els.play.disabled = false;
    els.signin.hidden = signedIn || !canSignIn;

    if (signedIn) {
      els.blurb.textContent = `Signed in as ${
        auth.profileName ?? "your account"
      }. Launching straight from HyPortal.`;
      els.playSub.textContent = "launch from HyPortal";
    } else if (canSignIn) {
      els.blurb.textContent =
        "Sign in with your Hytale account to launch the game directly from HyPortal.";
      els.signin.disabled = false;
      els.signinSub.textContent = "with your Hytale account";
      els.playSub.textContent = "sign in first";
    } else {
      els.blurb.textContent =
        "Host a private world for your friends from the panel on the right. Launching the game needs a client_id first.";
      els.playSub.textContent = "needs sign-in";
    }
    setNotice(null);
  } else {
    set(els.mStatus, "Not found", "bad");
    els.channelLine.textContent = " ";
    els.blurb.textContent =
      "HyPortal needs an existing Hytale installation. It never downloads or bundles game files.";
    els.play.disabled = true;
    els.signin.disabled = true;
    els.playSub.textContent = "unavailable";
    setNotice(install.problem);
  }
}

function renderHost() {
  if (!host) return;

  els.hostToggle.textContent = host.running ? "Stop world" : "Host a world";
  els.hostToggle.classList.toggle("on", host.running);
  els.hostToggle.disabled = !host.canHost && !host.running;

  set(els.mHost, host.running ? `Port ${host.port}` : "Not running", host.running ? "good" : "");

  // Authorising is optional and one-time; only offer it while a world is up.
  els.hostAuthorize.hidden = !host.running;

  els.hostConnect.hidden = !host.running;
  if (host.running) {
    renderAddresses();
    const r = host.reach;
    // Until the router has answered there is nothing useful to say, and an empty
    // line reads better than a spinner for something that resolves in seconds.
    els.hostDetail.textContent = r ? r.detail || "" : "Asking your router to open the port…";
    els.hostDetail.className = "host-connect-detail" + (r && !r.mapped ? " warn" : "");
  }

  const a = host.auth;
  const showAuth = host.running && a && (a.code || a.url);
  els.hostAuth.hidden = !showAuth;
  if (showAuth) {
    els.hostCode.textContent = a.code ?? "—";
    els.hostUrl.textContent = a.url ?? "";
    els.hostUrl.dataset.href = a.url ?? "";
  }

  els.hostLog.hidden = !host.log || host.log.length === 0;
  if (host.log && host.log.length) {
    els.hostLog.textContent = host.log.slice(-40).join("\n");
    els.hostLog.scrollTop = els.hostLog.scrollHeight;
  }

  if (!host.running && host.problem) setNotice(host.problem);
}

/// The three ways in, widest reach first. Each is a button that copies itself,
/// because the whole point is pasting it into Hytale's Add Server box.
function addressRows() {
  const port = host.port ?? (settings ? settings.port : 5520);
  const r = host.reach || {};
  const rows = [];

  if (r.mapped && r.externalAddress) {
    rows.push({ tag: "Friends anywhere", value: r.externalAddress });
  }
  if (r.localAddress) {
    rows.push({ tag: "Same Wi-Fi", value: r.localAddress });
  }
  rows.push({ tag: "This PC", value: `127.0.0.1:${port}` });
  return rows;
}

function renderAddresses() {
  const rows = addressRows();
  // Rebuilding on every poll would wipe the "copied" flash mid-animation.
  const key = rows.map((r) => `${r.tag}=${r.value}`).join("|");
  if (els.addrList.dataset.key === key) return;
  els.addrList.dataset.key = key;

  els.addrList.replaceChildren(
    ...rows.map((row) => {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "host-address";
      btn.title = "Click to copy";

      const tag = document.createElement("span");
      tag.className = "addr-tag";
      tag.textContent = row.tag;

      const value = document.createElement("span");
      value.className = "addr-value";
      value.textContent = row.value;

      btn.append(tag, value);
      btn.addEventListener("click", () => copyAddress(btn, row.value));
      return btn;
    })
  );
}

async function pollHost() {
  try {
    // The server runs `auth login device` itself via --boot-command, so we
    // only need to watch its console for the resulting code.
    host = await invoke("host_status");
    renderHost();
  } catch (err) {
    setNotice(`Host status failed: ${err}`);
  }
}

async function toggleHost() {
  els.hostToggle.disabled = true;
  try {
    if (host && host.running) {
      await invoke("host_stop");
    } else {
      setNotice(null);
      await invoke("host_start");
    }
    await pollHost();
  } catch (err) {
    setNotice(String(err));
  } finally {
    els.hostToggle.disabled = false;
  }
}

async function copyAddress(btn, text) {
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    // Clipboard API can be unavailable in a webview; fall back to a selection.
    const ta = document.createElement("textarea");
    ta.value = text;
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    ta.remove();
  }
  btn.classList.add("copied");
  setTimeout(() => btn.classList.remove("copied"), 1200);
}

async function authorizeHost() {
  els.hostAuthorize.disabled = true;
  els.hostAuthorize.textContent = "Requesting code…";
  try {
    await invoke("host_authorize");
  } catch (err) {
    setNotice(String(err));
  } finally {
    els.hostAuthorize.disabled = false;
    els.hostAuthorize.textContent = "Authorise for friends";
  }
}

// ---- world settings ----

// element id -> [settings key, kind]. One table, so a new setting is one line
// here plus one line of markup rather than two hand-written conversions.
const FIELDS = [
  ["sServerName", "serverName", "text"],
  ["sMotd", "motd", "text"],
  ["sPassword", "password", "text"],
  ["sMaxPlayers", "maxPlayers", "number"],
  ["sViewRadius", "viewRadius", "number"],
  ["sPort", "port", "number"],
  ["sWorldType", "worldType", "text"],
  ["sGameMode", "gameMode", "text"],
  ["sSeed", "seed", "text"],
  ["sCheats", "cheats", "bool"],
  ["sPvp", "pvp", "bool"],
  ["sFallDamage", "fallDamage", "bool"],
  ["sKeepInventory", "keepInventory", "bool"],
  ["sSpawnNpcs", "spawnNpcs", "bool"],
  ["sFreezeTime", "freezeTime", "bool"],
  ["sWhitelist", "whitelist", "bool"],
];

function fillSettingsForm() {
  if (!settings) return;
  for (const [id, key, kind] of FIELDS) {
    const el = $(id);
    if (kind === "bool") el.checked = Boolean(settings[key]);
    else el.value = settings[key] ?? "";
  }
  els.settingsLive.hidden = !(host && host.running);
  els.settingsRegen.hidden = !worldExists;
  els.settingsError.hidden = true;
}

function readSettingsForm() {
  const out = {};
  for (const [id, key, kind] of FIELDS) {
    const el = $(id);
    if (kind === "bool") out[key] = el.checked;
    else if (kind === "number") out[key] = Number(el.value) || 0;
    else out[key] = el.value;
  }
  return out;
}

function settingsFault(err) {
  els.settingsError.textContent = String(err);
  els.settingsError.hidden = false;
}

async function loadSettings() {
  try {
    const view = await invoke("world_settings");
    settings = view.settings;
    worldExists = view.worldExists;
    if (view.problem) settingsFault(view.problem);
  } catch (err) {
    settingsFault(err);
  }
}

async function openSettings() {
  await loadSettings();
  fillSettingsForm();
  els.modal.hidden = false;
}

function closeSettings() {
  els.modal.hidden = true;
}

async function saveSettings() {
  try {
    const view = await invoke("world_settings_save", { settings: readSettingsForm() });
    settings = view.settings;
    worldExists = view.worldExists;
    closeSettings();
    setNotice(
      host && host.running
        ? "Settings saved. Restart the world for them to take effect."
        : "Settings saved.",
      "ok"
    );
  } catch (err) {
    settingsFault(err);
  }
}

async function resetWorld() {
  if (host && host.running) {
    settingsFault("Stop the world before resetting it.");
    return;
  }
  // Deleting terrain someone has built in is not a thing to do on one click.
  if (!window.confirm("Delete the hosted world and everything built in it? This cannot be undone.")) {
    return;
  }
  try {
    const view = await invoke("world_reset");
    settings = view.settings;
    worldExists = view.worldExists;
    fillSettingsForm();
    setNotice("World deleted. The next start generates a fresh one.", "ok");
  } catch (err) {
    settingsFault(err);
  }
}

async function signIn() {
  els.signin.disabled = true;
  els.signinSub.textContent = "check your browser…";
  try {
    auth = await invoke("sign_in");
    render();
  } catch (err) {
    setNotice(String(err));
    els.signin.disabled = false;
    els.signinSub.textContent = "try again";
  }
}

async function refresh() {
  els.refresh.disabled = true;
  try {
    [install, auth] = await Promise.all([
      invoke("detect_install"),
      invoke("auth_status"),
    ]);
    render();
  } catch (err) {
    setNotice(`Detection failed: ${err}`);
  } finally {
    els.refresh.disabled = false;
  }
}

async function play() {
  els.play.disabled = true;
  els.playSub.textContent = "starting…";
  try {
    const msg = await invoke("launch_game");
    setNotice(`${msg} Finish signing in there — the game will start automatically.`, "ok");
    els.playSub.textContent = "launcher open";
  } catch (err) {
    setNotice(String(err));
    els.play.disabled = false;
    els.playSub.textContent = "try again";
  }
}

els.play.addEventListener("click", play);
els.signin.addEventListener("click", signIn);
els.refresh.addEventListener("click", refresh);
els.hostToggle.addEventListener("click", toggleHost);
els.hostSettings.addEventListener("click", openSettings);
els.hostAuthorize.addEventListener("click", authorizeHost);

$("settingsClose").addEventListener("click", closeSettings);
$("settingsCancel").addEventListener("click", closeSettings);
$("settingsSave").addEventListener("click", saveSettings);
$("settingsReset").addEventListener("click", resetWorld);
// Clicking the backdrop dismisses; clicking the card itself must not.
els.modal.addEventListener("click", (e) => {
  if (e.target === els.modal) closeSettings();
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && !els.modal.hidden) closeSettings();
});

refresh();
loadSettings();
pollHost();
hostTimer = setInterval(pollHost, 1500);
window.addEventListener("beforeunload", () => clearInterval(hostTimer));
