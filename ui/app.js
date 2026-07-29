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
els.hostAuthorize = $("hostAuthorize");
els.hostConnect = $("hostConnect");
els.hostAddress = $("hostAddress");
els.hostDetail = $("hostDetail");
els.hostAuth = $("hostAuth");
els.hostCode = $("hostCode");
els.hostUrl = $("hostUrl");
els.hostLog = $("hostLog");
els.mHost = $("mHost");

let install = null;
let auth = null;
let host = null;
let hostTimer = null;

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

  const r = host.reach;
  els.hostConnect.hidden = !host.running || !r;
  if (host.running && r) {
    // Prefer the internet-reachable address; fall back to the LAN one.
    const addr = (r.mapped && r.externalAddress) || r.localAddress || "—";
    if (els.hostAddress.textContent !== addr) {
      els.hostAddress.textContent = addr;
      els.hostAddress.classList.remove("copied");
    }
    els.hostDetail.textContent = r.detail || "";
    els.hostDetail.className =
      "host-connect-detail" + (r.mapped ? "" : " warn");
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
      await invoke("host_start", { port: 5520 });
    }
    await pollHost();
  } catch (err) {
    setNotice(String(err));
  } finally {
    els.hostToggle.disabled = false;
  }
}

async function copyAddress() {
  const text = els.hostAddress.textContent.trim();
  if (!text || text === "—") return;
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
  els.hostAddress.classList.add("copied");
  setTimeout(() => els.hostAddress.classList.remove("copied"), 1200);
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
els.hostAuthorize.addEventListener("click", authorizeHost);
els.hostAddress.addEventListener("click", copyAddress);

refresh();
pollHost();
hostTimer = setInterval(pollHost, 1500);
window.addEventListener("beforeunload", () => clearInterval(hostTimer));
