//! Hosting a world for friends.
//!
//! HyPortal runs the *integrated server jar that ships with Hytale* — the same
//! `HytaleServer.jar` the official client spawns for singleplayer worlds. It is
//! already on disk, so nothing is downloaded or redistributed.
//!
//! Authentication is the server's own job: it exposes an `/auth login device`
//! console command that performs the OAuth device flow against Hypixel's
//! provider using the public `hytale-server` client. HyPortal drives that console
//! and surfaces the verification URL and user code in the UI. We never see a
//! credential, and we need no client_id of our own.

use serde::Serialize;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

/// Keep the console scrollback bounded; a long session would otherwise grow
/// without limit.
const MAX_LOG_LINES: usize = 600;

#[derive(Default)]
pub struct Host {
    inner: Mutex<Option<Running>>,
    log: Arc<Mutex<Vec<String>>>,
    auth: Arc<Mutex<Option<DeviceAuth>>>,
    reach: Arc<Mutex<Option<crate::net::Reachability>>>,
}

struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    port: u16,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAuth {
    pub url: Option<String>,
    pub code: Option<String>,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HostStatus {
    pub can_host: bool,
    pub running: bool,
    pub port: Option<u16>,
    pub auth: Option<DeviceAuth>,
    pub reach: Option<crate::net::Reachability>,
    pub log: Vec<String>,
    pub problem: Option<String>,
}

/// Everything needed to build the java command line.
struct Paths {
    java: PathBuf,
    jar: PathBuf,
    assets: PathBuf,
    worlds: PathBuf,
}

fn paths() -> Result<Paths, String> {
    let install = crate::hytale::detect();

    let missing = |what: &str| {
        format!("Can't host yet — {what} is missing. Open the official launcher once so Hytale finishes downloading.")
    };

    Ok(Paths {
        java: install.java_exec.ok_or_else(|| {
            "Can't host — no Java runtime found. Hytale normally bundles one; \
             reinstall via the official launcher."
                .to_string()
        })?.into(),
        jar: install.server_jar.ok_or_else(|| missing("the server files"))?.into(),
        assets: install.assets_zip.ok_or_else(|| missing("the game assets"))?.into(),
        worlds: PathBuf::from(install.user_data.ok_or_else(|| missing("the user data folder"))?)
            .join("HyPortalWorlds"),
    })
}

/// The server colours its console output; strip SGR escapes before we parse or
/// display a line.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Consume up to and including the terminating letter of the sequence.
            for e in chars.by_ref() {
                if e.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Scan a console line for the device-flow prompt so the UI can show it.
///
/// Anchored on the server's actual wording, verified against a live run:
///
/// ```text
/// [AbstractCommand] Visit: https://oauth.accounts.hytale.com/oauth2/device/verify
/// [AbstractCommand] Enter code: gQwTp6P3
/// ```
///
/// Codes are mixed-case alphanumerics, so no case assumptions.
fn scan_for_auth(line: &str, auth: &Arc<Mutex<Option<DeviceAuth>>>) {
    let has_code = line.contains("Enter code:");
    let has_url = line.contains("https://") && line.contains("oauth.accounts.hytale.com");
    if !has_code && !has_url {
        return;
    }

    let mut guard = auth.lock().unwrap();
    let entry = guard.get_or_insert_with(DeviceAuth::default);

    if has_code {
        if let Some(rest) = line.split("Enter code:").nth(1) {
            let code = rest.trim();
            if !code.is_empty() {
                entry.code = Some(code.to_string());
            }
        }
    }

    if has_url {
        if let Some(start) = line.find("https://") {
            let url: String = line[start..]
                .chars()
                .take_while(|c| !c.is_whitespace())
                .collect();
            // Prefer the prefilled variant, which saves the user typing the code.
            let better = url.contains("user_code=");
            if better || entry.url.is_none() {
                entry.url = Some(url);
            }
        }
    }
}

fn push_log(log: &Arc<Mutex<Vec<String>>>, line: String) {
    let mut guard = log.lock().unwrap();
    guard.push(line);
    if guard.len() > MAX_LOG_LINES {
        let excess = guard.len() - MAX_LOG_LINES;
        guard.drain(0..excess);
    }
}

/// Pump one of the child's output streams into the shared log.
fn pump<R: std::io::Read + Send + 'static>(
    stream: R,
    log: Arc<Mutex<Vec<String>>>,
    auth: Arc<Mutex<Option<DeviceAuth>>>,
) {
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            let line = strip_ansi(&line);
            scan_for_auth(&line, &auth);
            push_log(&log, line);
        }
    });
}

impl Host {
    pub fn status(&self) -> HostStatus {
        let mut out = HostStatus {
            log: self.log.lock().unwrap().clone(),
            auth: self.auth.lock().unwrap().clone(),
            reach: self.reach.lock().unwrap().clone(),
            ..Default::default()
        };

        match paths() {
            Ok(_) => out.can_host = true,
            Err(problem) => out.problem = Some(problem),
        }

        let mut guard = self.inner.lock().unwrap();
        if let Some(run) = guard.as_mut() {
            match run.child.try_wait() {
                // Still alive.
                Ok(None) => {
                    out.running = true;
                    out.port = Some(run.port);
                }
                // Exited on its own; clear the slot so Start works again.
                Ok(Some(code)) => {
                    push_log(&self.log, format!("[HyPortal] server exited: {code}"));
                    *guard = None;
                }
                Err(e) => out.problem = Some(format!("Lost track of the server process: {e}")),
            }
        }
        out
    }

    pub fn start(&self, port: u16) -> Result<(), String> {
        let mut guard = self.inner.lock().unwrap();
        if guard.is_some() {
            return Err("A world is already running.".into());
        }

        let p = paths()?;
        std::fs::create_dir_all(&p.worlds)
            .map_err(|e| format!("Could not create the worlds folder: {e}"))?;

        self.log.lock().unwrap().clear();
        *self.auth.lock().unwrap() = None;
        *self.reach.lock().unwrap() = None;
        push_log(&self.log, format!("[HyPortal] starting server on UDP {port}"));

        // UPnP discovery involves multicast and a round trip to the router, so
        // do it off the calling thread — the world shouldn't wait on the LAN.
        {
            let reach = self.reach.clone();
            let log = self.log.clone();
            std::thread::spawn(move || {
                let result = crate::net::open_port(port);
                push_log(&log, format!("[HyPortal] {}", result.detail));
                *reach.lock().unwrap() = Some(result);
            });
        }

        // Deliberately no `--boot-command auth login device` here. The server
        // runs fine unauthenticated — that is exactly how the official client
        // starts singleplayer worlds — and forcing the device flow on every
        // start would make you re-verify just to play alone. Authorising is
        // opt-in via `authorize()`, and the server's encrypted credential
        // store keeps it for next time.
        let mut cmd = Command::new(&p.java);
        cmd.arg("-cp")
            .arg(&p.jar)
            .arg("com.hypixel.hytale.Main")
            .arg("--allow-op")
            .arg("--disable-sentry")
            .arg(format!("--assets={}", p.assets.display()))
            .arg("--universe")
            .arg(&p.worlds)
            .arg("--bind")
            .arg(format!("0.0.0.0:{port}"))
            .arg("--transport")
            .arg("QUIC");

        // Make the signed-in account the world owner when we know who that is.
        let install = crate::hytale::detect();
        if let (Some(name), Some(uuid)) = (&install.profile_name, &install.profile_uuid) {
            cmd.arg("--owner-name").arg(name).arg("--owner-uuid").arg(uuid);
        }

        let mut child = cmd
            .current_dir(&p.worlds)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Could not start the server: {e}"))?;

        if let Some(out) = child.stdout.take() {
            pump(out, self.log.clone(), self.auth.clone());
        }
        if let Some(err) = child.stderr.take() {
            pump(err, self.log.clone(), self.auth.clone());
        }

        let stdin = child.stdin.take();
        *guard = Some(Running { child, stdin, port });
        Ok(())
    }

    /// Begin the one-time device-flow authorisation for this world. Only needed
    /// if you want friends joining with verified Hytale accounts; solo play does
    /// not require it. Credentials persist in the server's encrypted store, so
    /// this should be a once-per-world action, not once-per-launch.
    pub fn authorize(&self) -> Result<(), String> {
        *self.auth.lock().unwrap() = None;
        // Console commands take no leading slash.
        self.send("auth login device")
    }

    /// Write a line to the server console.
    pub fn send(&self, line: &str) -> Result<(), String> {
        let mut guard = self.inner.lock().unwrap();
        let run = guard.as_mut().ok_or("No world is running.")?;
        let stdin = run.stdin.as_mut().ok_or("Server console is not writable.")?;
        writeln!(stdin, "{line}").map_err(|e| format!("Could not write to console: {e}"))?;
        stdin.flush().map_err(|e| format!("Console flush failed: {e}"))?;
        push_log(&self.log, format!("> {line}"));
        Ok(())
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut guard = self.inner.lock().unwrap();
        let Some(run) = guard.as_mut() else {
            return Err("No world is running.".into());
        };

        // Tidy up the router mapping; harmless if there wasn't one.
        let port = run.port;
        let reach = self.reach.clone();
        std::thread::spawn(move || {
            crate::net::close_port(port);
            *reach.lock().unwrap() = None;
        });

        // Ask politely first; the server flushes worlds on shutdown.
        if let Some(stdin) = run.stdin.as_mut() {
            let _ = writeln!(stdin, "stop");
            let _ = stdin.flush();
        }

        for _ in 0..40 {
            if matches!(run.child.try_wait(), Ok(Some(_))) {
                *guard = None;
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }

        run.child
            .kill()
            .map_err(|e| format!("Could not stop the server: {e}"))?;
        *guard = None;
        Ok(())
    }
}
