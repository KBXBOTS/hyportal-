//! Discovery and launching of a local Hytale installation.
//!
//! Nothing here reads `account.dat` or `.keys/`. Those hold credential material
//! belonging to the official launcher; HyPortal deliberately stays out of them.
//! Profile identity is recovered from the launcher's own plaintext log instead,
//! and authentication is delegated to the official launcher until we have our
//! own OAuth client_id (see `docs/AUTH.md`).

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Where the official launcher keeps its application directory, per platform.
///
/// Windows is verified against a real install. The macOS and Linux paths follow
/// each platform's convention and are best-effort until confirmed on hardware.
fn app_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("Hytale"))
    }

    #[cfg(target_os = "macos")]
    {
        home().map(|h| h.join("Library/Application Support/Hytale"))
    }

    #[cfg(target_os = "linux")]
    {
        // Respect XDG_DATA_HOME, then fall back through the usual suspects.
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            let p = PathBuf::from(xdg).join("Hytale");
            if p.is_dir() {
                return Some(p);
            }
        }
        let home = home()?;
        [".local/share/Hytale", ".config/Hytale", ".hytale"]
            .iter()
            .map(|rel| home.join(rel))
            .find(|p| p.is_dir())
            .or_else(|| Some(home.join(".local/share/Hytale")))
    }
}

#[cfg(not(target_os = "windows"))]
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// The official launcher executable, which we shell out to for sign-in.
fn launcher_exe() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let base = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
        let exe = base.join(r"Programs\Hypixel Studios\Hytale Launcher\hytale-launcher.exe");
        exe.is_file().then_some(exe)
    }

    #[cfg(target_os = "macos")]
    {
        let candidates = [
            PathBuf::from("/Applications/Hytale Launcher.app"),
            home()?.join("Applications/Hytale Launcher.app"),
        ];
        candidates.into_iter().find(|p| p.exists())
    }

    #[cfg(target_os = "linux")]
    {
        let mut candidates = vec![
            PathBuf::from("/usr/bin/hytale-launcher"),
            PathBuf::from("/usr/local/bin/hytale-launcher"),
            PathBuf::from("/opt/hytale-launcher/hytale-launcher"),
        ];
        if let Some(h) = home() {
            candidates.push(h.join(".local/bin/hytale-launcher"));
        }
        candidates.into_iter().find(|p| p.is_file())
    }
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Install {
    pub found: bool,
    pub app_dir: Option<String>,
    pub user_data: Option<String>,
    pub patchline: Option<String>,
    pub client_exe: Option<String>,
    pub launcher_exe: Option<String>,
    pub server_jar: Option<String>,
    pub java_exec: Option<String>,
    pub assets_zip: Option<String>,
    pub game_version: Option<String>,
    pub game_build: Option<String>,
    pub launcher_version: Option<String>,
    pub profile_name: Option<String>,
    pub profile_uuid: Option<String>,
    pub server_count: Option<usize>,
    /// Why detection came up empty, when it did.
    pub problem: Option<String>,
}

/// Read `patchline.json`, which names the active channel and user-data dir.
fn read_patchline(app: &Path) -> (Option<String>, Option<String>) {
    let Ok(raw) = std::fs::read_to_string(app.join("patchline.json")) else {
        return (None, None);
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return (None, None);
    };
    (
        v.get("patchline").and_then(|x| x.as_str()).map(String::from),
        v.get("user_data").and_then(|x| x.as_str()).map(String::from),
    )
}

/// Pull the last `key=value` occurrence out of a log line body.
fn field<'a>(hay: &'a str, key: &str) -> Option<&'a str> {
    let start = hay.rfind(key)? + key.len();
    let rest = &hay[start..];
    let end = rest
        .find(|c: char| c == ',' || c == ' ' || c == '|')
        .unwrap_or(rest.len());
    let val = rest[..end].trim();
    (!val.is_empty() && val != "<nil>").then_some(val)
}

/// Scrape non-secret facts out of the launcher's plaintext log: which profile
/// signed in last, which game build ran, which launcher version is installed.
///
/// The log also contains a truncated session-token *summary* line; we never read
/// or retain it. We only look at the four keys below.
struct LogFacts {
    profile_name: Option<String>,
    profile_uuid: Option<String>,
    game_version: Option<String>,
    game_build: Option<String>,
    launcher_version: Option<String>,
}

fn scrape_log(app: &Path) -> LogFacts {
    let mut facts = LogFacts {
        profile_name: None,
        profile_uuid: None,
        game_version: None,
        game_build: None,
        launcher_version: None,
    };

    let Ok(raw) = std::fs::read_to_string(app.join("hytale-launcher.log")) else {
        return facts;
    };

    // Later lines win, so a re-login or update is reflected immediately.
    for line in raw.lines() {
        if let Some(idx) = line.find("Auth config:") {
            let body = &line[idx..];
            if let Some(n) = field(body, "name=") {
                facts.profile_name = Some(n.to_string());
            }
            if let Some(u) = field(body, "uuid=") {
                facts.profile_uuid = Some(u.to_string());
            }
        }
        if line.contains("starting game process") {
            if let Some(v) = field(line, "game_version=") {
                facts.game_version = Some(v.to_string());
            }
            if let Some(b) = field(line, "game_build=") {
                facts.game_build = Some(b.to_string());
            }
        }
        if line.contains("starting hytale-launcher") {
            if let Some(v) = field(line, "version=") {
                facts.launcher_version = Some(v.to_string());
            }
        }
    }
    facts
}

/// Count saved servers, so the UI can show something useful about multiplayer.
fn count_servers(user_data: &Path) -> Option<usize> {
    let raw = std::fs::read_to_string(user_data.join("ServerList.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    match v {
        serde_json::Value::Array(a) => Some(a.len()),
        serde_json::Value::Object(ref o) => o
            .values()
            .find_map(|x| x.as_array().map(|a| a.len()))
            .or(Some(0)),
        _ => None,
    }
}

/// Root of the installed package for a channel: `install/<patchline>/package`.
pub fn package_root(app: &Path, patchline: &str) -> PathBuf {
    app.join("install").join(patchline).join("package")
}

/// The integrated server jar. The official client launches exactly this to run
/// singleplayer worlds, so it is always present alongside the client.
pub fn server_jar(app: &Path, patchline: &str) -> Option<PathBuf> {
    let p = package_root(app, patchline)
        .join("game")
        .join("latest")
        .join("Server")
        .join("HytaleServer.jar");
    p.is_file().then_some(p)
}

pub fn assets_zip(app: &Path, patchline: &str) -> Option<PathBuf> {
    let p = package_root(app, patchline)
        .join("game")
        .join("latest")
        .join("Assets.zip");
    p.is_file().then_some(p)
}

/// Prefer the JRE bundled with Hytale; fall back to whatever `java` is on PATH.
pub fn java_exec(app: &Path, patchline: &str) -> Option<PathBuf> {
    let bin = package_root(app, patchline).join("jre").join("latest").join("bin");
    let bundled = if cfg!(windows) {
        bin.join("java.exe")
    } else {
        bin.join("java")
    };
    if bundled.is_file() {
        return Some(bundled);
    }

    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(exe))
            .find(|p| p.is_file())
    })
}

fn client_binary(app: &Path, patchline: &str) -> Option<PathBuf> {
    let dir = app
        .join("install")
        .join(patchline)
        .join("package")
        .join("game")
        .join("latest")
        .join("Client");

    let exe = if cfg!(windows) {
        dir.join("HytaleClient.exe")
    } else {
        dir.join("HytaleClient")
    };
    exe.is_file().then_some(exe)
}

pub fn detect() -> Install {
    let mut out = Install::default();

    let Some(app) = app_dir() else {
        out.problem = Some("Could not determine this platform's Hytale data directory.".into());
        return out;
    };
    out.app_dir = Some(app.display().to_string());
    out.launcher_exe = launcher_exe().map(|p| p.display().to_string());

    if !app.is_dir() {
        out.problem = Some(format!(
            "No Hytale installation at {}. Install Hytale with the official launcher first.",
            app.display()
        ));
        return out;
    }

    let (patchline, user_data) = read_patchline(&app);
    let patchline = patchline.unwrap_or_else(|| "release".to_string());

    let user_data_path = user_data
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| app.join("UserData"));
    out.server_count = count_servers(&user_data_path);
    out.user_data = Some(user_data_path.display().to_string());

    let facts = scrape_log(&app);
    out.profile_name = facts.profile_name;
    out.profile_uuid = facts.profile_uuid;
    out.game_version = facts.game_version;
    out.game_build = facts.game_build;
    out.launcher_version = facts.launcher_version;

    out.server_jar = server_jar(&app, &patchline).map(|p| p.display().to_string());
    out.java_exec = java_exec(&app, &patchline).map(|p| p.display().to_string());
    out.assets_zip = assets_zip(&app, &patchline).map(|p| p.display().to_string());

    match client_binary(&app, &patchline) {
        Some(exe) => {
            out.client_exe = Some(exe.display().to_string());
            out.found = true;
        }
        None => {
            out.problem = Some(format!(
                "Found the Hytale data folder but no game client on the '{patchline}' channel. \
                 Open the official launcher once to finish downloading."
            ));
        }
    }

    out.patchline = Some(patchline);
    out
}

/// Launch `HytaleClient.exe` directly, with no official launcher involved.
///
/// This is the flow the client demands in authenticated mode — it refuses to
/// start without both JWTs:
///
/// ```text
/// ERROR  Authenticated mode requires --identity-token and --session-token
/// ```
pub fn launch_direct(session: &crate::auth::GameSession) -> Result<String, String> {
    let install = detect();

    let client = install
        .client_exe
        .ok_or("Hytale client not found. Open the official launcher once to finish downloading.")?;
    let app_dir = install.app_dir.ok_or("Hytale data folder not found.")?;
    let user_dir = install.user_data.ok_or("Hytale user data folder not found.")?;
    let patchline = install.patchline.unwrap_or_else(|| "release".into());

    let game_root = std::path::Path::new(&app_dir)
        .join("install")
        .join(&patchline)
        .join("package")
        .join("game")
        .join("latest");

    let mut cmd = std::process::Command::new(&client);
    cmd.arg("--app-dir")
        .arg(&game_root)
        .arg("--user-dir")
        .arg(&user_dir)
        .arg("--auth-mode")
        .arg("authenticated")
        .arg("--uuid")
        .arg(&session.uuid)
        .arg("--name")
        .arg(&session.name)
        .arg("--session-token")
        .arg(&session.session_token)
        .arg("--identity-token")
        .arg(&session.identity_token);

    if let Some(java) = install.java_exec {
        cmd.arg("--java-exec").arg(java);
    }

    cmd.spawn()
        .map(|child| format!("Hytale started (pid {}).", child.id()))
        .map_err(|e| format!("Could not start Hytale: {e}"))
}

/// Start the game.
///
/// Today this delegates to the official launcher, which owns the OAuth flow and
/// mints the `hytale:client` session token the client requires. HyPortal never
/// handles a credential. Once we hold our own client_id we can authenticate
/// in-app and spawn `client_exe` directly; the UI contract does not change.
pub fn launch() -> Result<String, String> {
    // HyPortal never opens the official launcher. Either we hold a session and
    // spawn the client ourselves, or we say what's missing and stop.
    let Some(session) = crate::auth::current_session() else {
        return Err(
            "Not signed in, so there's no session token to start the game with. \
             HytaleClient.exe refuses to launch in authenticated mode without one. \
             Set HYPORTAL_CLIENT_ID (or create client_id.txt next to HyPortal.exe) and sign in first."
                .into(),
        );
    };

    launch_direct(&session)
}
