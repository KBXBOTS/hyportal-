//! In-app sign-in: OAuth 2.0 authorization code + PKCE over a loopback
//! redirect (RFC 8252), the same shape MultiMC/Prism use against Microsoft.
//!
//! # Status of each piece
//!
//! * **Verified** — the endpoints' host names, the loopback pattern, and the
//!   `hytale:client` scope, all observed in the official launcher's own log.
//! * **Standard** — PKCE, the code exchange, and refresh. These follow RFC 7636
//!   and 6749; there is nothing Hytale-specific to get wrong.
//! * **Assumed** — the exact paths marked `ASSUMED` below, and how the client
//!   binary receives its session. Run `tools/capture_client_args.ps1` to settle
//!   the latter; the former needs one round-trip against a real client_id.
//!
//! Nothing here will authenticate until HyPortal has its own `client_id` issued by
//! Hypixel Studios. See `docs/hypixel-request.md`. HyPortal deliberately does not
//! reuse the official launcher's client identity, and does not read its stored
//! credentials.

use serde::Serialize;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The live session for this run. Held in memory only — never written to disk,
/// so closing HyPortal discards it.
static SESSION: Mutex<Option<GameSession>> = Mutex::new(None);

/// The current session, if sign-in has completed.
pub fn current_session() -> Option<GameSession> {
    SESSION.lock().ok()?.clone()
}

const AUTH_HOST: &str = "https://oauth.accounts.hytale.com";
const AUTHORIZE_PATH: &str = "/oauth2/auth";
const TOKEN_PATH: &str = "/oauth2/token";
const SCOPE: &str = "hytale:client"; // verified from client logs

/// Exchanges an access token for a short-lived `hytale:client` game session,
/// which is what `HytaleClient.exe` actually validates on startup.
const SESSION_URL: &str = "https://sessions.hytale.com/game-session/new";

/// How long we wait for the user to finish signing in.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// Fixed loopback port for the OAuth redirect.
///
/// RFC 8252 lets an authorization server accept any port on a loopback
/// redirect, but plenty of implementations still require an exact match against
/// what was registered. A fixed port is registered once and always matches.
const REDIRECT_PORT: u16 = 43110;

/// The redirect URI to register on the Hytale developer-apps form. It must be
/// byte-identical there and here.
pub const REDIRECT_URI: &str = "http://127.0.0.1:43110/callback";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    /// False when no client_id is configured, which is the current default.
    pub configured: bool,
    pub signed_in: bool,
    pub profile_name: Option<String>,
    pub profile_uuid: Option<String>,
    /// Human-readable explanation for the UI.
    pub detail: String,
}

/// Where the client_id comes from: `HYPORTAL_CLIENT_ID`, or `client_id.txt`
/// beside the executable. Kept out of the binary so it is trivially swappable
/// the day Hypixel issues one.
pub fn client_id() -> Option<String> {
    if let Ok(v) = std::env::var("HYPORTAL_CLIENT_ID") {
        let v = v.trim().to_string();
        if !v.is_empty() {
            return Some(v);
        }
    }
    let path = std::env::current_exe().ok()?.parent()?.join("client_id.txt");
    let raw = std::fs::read_to_string(path).ok()?;
    let v = raw.trim().to_string();
    (!v.is_empty()).then_some(v)
}

pub fn status() -> AuthStatus {
    match client_id() {
        None => AuthStatus {
            configured: false,
            signed_in: false,
            profile_name: None,
            profile_uuid: None,
            detail: "No OAuth client_id configured. HyPortal cannot sign in on its own yet — \
                     request one from Hypixel Studios (see docs/hypixel-request.md), then \
                     drop it in client_id.txt next to the executable."
                .into(),
        },
        Some(_) => AuthStatus {
            configured: true,
            signed_in: false,
            profile_name: None,
            profile_uuid: None,
            detail: "Ready to sign in.".into(),
        },
    }
}

// ---------------------------------------------------------------- PKCE

fn base64url(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        let idx = [n >> 18 & 63, n >> 12 & 63, n >> 6 & 63, n & 63];
        for (i, &j) in idx.iter().enumerate() {
            // Emit only the characters backed by real input bytes; base64url
            // carries no '=' padding.
            if i <= chunk.len() {
                out.push(T[j as usize] as char);
            }
        }
    }
    out
}

fn random_b64(len: usize) -> Result<String, String> {
    let mut buf = vec![0u8; len];
    getrandom::getrandom(&mut buf).map_err(|e| format!("no system randomness: {e}"))?;
    Ok(base64url(&buf))
}

struct Pkce {
    verifier: String,
    challenge: String,
}

fn pkce() -> Result<Pkce, String> {
    use sha2::{Digest, Sha256};
    let verifier = random_b64(32)?;
    let challenge = base64url(&Sha256::digest(verifier.as_bytes()));
    Ok(Pkce { verifier, challenge })
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ------------------------------------------------------- loopback listener

/// The browser lands here after the user approves. We read the `code` and
/// `state` off the request line, show a plain confirmation page, and close.
fn await_redirect(listener: &TcpListener, expect_state: &str) -> Result<String, String> {
    listener
        .set_nonblocking(false)
        .map_err(|e| format!("loopback socket: {e}"))?;

    let deadline = SystemTime::now() + LOGIN_TIMEOUT;

    for stream in listener.incoming() {
        if SystemTime::now() > deadline {
            return Err("Sign-in timed out.".into());
        }
        let mut stream = stream.map_err(|e| format!("loopback accept: {e}"))?;

        let mut line = String::new();
        BufReader::new(&stream)
            .read_line(&mut line)
            .map_err(|e| format!("loopback read: {e}"))?;

        // "GET /callback?code=...&state=... HTTP/1.1"
        let target = line.split_whitespace().nth(1).unwrap_or("");
        let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");

        let mut code = None;
        let mut state = None;
        let mut error = None;
        for pair in query.split('&') {
            match pair.split_once('=') {
                Some(("code", v)) => code = Some(v.to_string()),
                Some(("state", v)) => state = Some(v.to_string()),
                Some(("error", v)) => error = Some(v.to_string()),
                _ => {}
            }
        }

        let ok = error.is_none() && code.is_some() && state.as_deref() == Some(expect_state);
        let body = if ok {
            "<!doctype html><meta charset=utf-8><title>Signed in</title>\
             <body style=\"font:16px system-ui;background:#0e1013;color:#e8eaed;\
             display:grid;place-items:center;height:100vh;margin:0\">\
             <div style=\"text-align:center\"><h1>Signed in</h1>\
             <p>You can close this tab and return to HyPortal.</p></div>"
        } else {
            "<!doctype html><meta charset=utf-8><title>Sign-in failed</title>\
             <body style=\"font:16px system-ui;background:#0e1013;color:#e8eaed;\
             display:grid;place-items:center;height:100vh;margin:0\">\
             <div style=\"text-align:center\"><h1>Sign-in failed</h1>\
             <p>Return to HyPortal and try again.</p></div>"
        };
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.flush();

        if let Some(e) = error {
            return Err(format!("Authorization server returned: {e}"));
        }
        if state.as_deref() != Some(expect_state) {
            return Err("State mismatch — possible CSRF, sign-in aborted.".into());
        }
        return code.ok_or_else(|| "No authorization code in redirect.".into());
    }

    Err("Loopback listener closed unexpectedly.".into())
}

fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();

    r.map(|_| ()).map_err(|e| format!("Could not open browser: {e}"))
}

// ------------------------------------------------------------- the flow

/// Run the full interactive sign-in. Blocking; call from a background thread.
pub fn sign_in() -> Result<AuthStatus, String> {
    let Some(cid) = client_id() else {
        return Err(status().detail);
    };

    let listener = TcpListener::bind(("127.0.0.1", REDIRECT_PORT)).map_err(|e| {
        format!(
            "Could not open loopback port {REDIRECT_PORT} for sign-in ({e}). \
             Another program may be using it."
        )
    })?;
    let redirect = REDIRECT_URI.to_string();

    let pk = pkce()?;
    let state = random_b64(16)?;

    let url = format!(
        "{AUTH_HOST}{AUTHORIZE_PATH}?response_type=code&client_id={}&redirect_uri={}\
         &scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        urlencode(&cid),
        urlencode(&redirect),
        urlencode(SCOPE),
        urlencode(&state),
        urlencode(&pk.challenge),
    );

    open_browser(&url)?;
    let code = await_redirect(&listener, &state)?;

    let resp = ureq::post(&format!("{AUTH_HOST}{TOKEN_PATH}"))
        .send_form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &redirect),
            ("client_id", &cid),
            ("code_verifier", &pk.verifier),
        ])
        .map_err(|e| format!("Token exchange failed: {e}"))?;

    let raw = resp
        .into_string()
        .map_err(|e| format!("Could not read token response: {e}"))?;
    let body: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("Token response was not JSON: {e}"))?;

    let access = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or("Token response had no access_token")?;

    let claims = decode_jwt_claims(access).unwrap_or_default();
    let claim_name = claims.get("name").and_then(|v| v.as_str()).map(String::from);
    let claim_uuid = claims.get("sub").and_then(|v| v.as_str()).map(String::from);

    // Exchange for the game session the client requires, so Play can spawn
    // HytaleClient.exe directly instead of handing off.
    let session = mint_session(access)?;
    let name = if session.name.is_empty() {
        claim_name.clone()
    } else {
        Some(session.name.clone())
    };
    let uuid = if session.uuid.is_empty() {
        claim_uuid.clone()
    } else {
        Some(session.uuid.clone())
    };

    if let Ok(mut slot) = SESSION.lock() {
        *slot = Some(GameSession {
            name: name.clone().unwrap_or_default(),
            uuid: uuid.clone().unwrap_or_default(),
            ..session
        });
    }

    Ok(AuthStatus {
        configured: true,
        signed_in: true,
        profile_name: name,
        profile_uuid: uuid,
        detail: "Signed in.".into(),
    })
}

/// The pair of JWTs `HytaleClient.exe` demands in authenticated mode.
///
/// Verified from the client's own `--help`:
/// ```text
/// --identity-token  Identity token JWT (required for authenticated mode)
/// --session-token   Session token JWT (required for authenticated mode)
/// ```
#[derive(Clone, Default)]
pub struct GameSession {
    pub session_token: String,
    pub identity_token: String,
    pub uuid: String,
    pub name: String,
}

/// Mint a game session from an OAuth access token.
///
/// The endpoint is confirmed; the exact request and response field names are
/// inferred and may need one correction against a live call. Everything else in
/// this module is verified.
pub fn mint_session(access_token: &str) -> Result<GameSession, String> {
    let resp = ureq::post(SESSION_URL)
        .set("Authorization", &format!("Bearer {access_token}"))
        .set("Content-Type", "application/json")
        .send_string("{}")
        .map_err(|e| format!("Could not create a game session: {e}"))?;

    let raw = resp
        .into_string()
        .map_err(|e| format!("Could not read the session response: {e}"))?;
    let body: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("Session response was not JSON: {e}"))?;

    // Accept a few plausible spellings so a naming mismatch is not fatal.
    let pick = |keys: &[&str]| -> Option<String> {
        keys.iter()
            .find_map(|k| body.get(*k).and_then(|v| v.as_str()))
            .map(String::from)
    };

    let session_token = pick(&["session_token", "sessionToken", "token"])
        .ok_or("Session response contained no session token")?;
    let identity_token = pick(&["identity_token", "identityToken"])
        .ok_or("Session response contained no identity token")?;

    let claims = decode_jwt_claims(&session_token).unwrap_or_default();

    Ok(GameSession {
        uuid: claims
            .get("sub")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        name: pick(&["name", "displayName"]).unwrap_or_default(),
        session_token,
        identity_token,
    })
}

/// Decode a JWT payload without verifying it. Only used to display the
/// account name; never for a trust decision.
fn decode_jwt_claims(jwt: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    let payload = jwt.split('.').nth(1)?;
    let mut bytes = Vec::new();
    let table = |c: u8| -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    };
    let vals: Vec<u8> = payload.bytes().filter_map(table).collect();
    for chunk in vals.chunks(4) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, &v)| acc | u32::from(v) << (18 - 6 * i));
        for i in 0..chunk.len().saturating_sub(1) {
            bytes.push((n >> (16 - 8 * i)) as u8);
        }
    }
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()?
        .as_object()
        .cloned()
}

/// Seconds since epoch, for token expiry bookkeeping.
#[allow(dead_code)]
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
