<div align="center">
  <img src="src-tauri/icons/128x128.png" width="96" alt="HyPortal">
  <h1>HyPortal</h1>
  <p><em>An unofficial, community-built launcher for Hytale.</em></p>
</div>

> **Not affiliated with, endorsed by, or associated with Hypixel Studios or Riot
> Games.** Hytale must be purchased and installed through official channels.
> HyPortal does not bundle, download, or redistribute any game or server files —
> it only launches what is already installed.

## What it does

**Host a world for your friends.** One click starts the Hytale server that
already ships with your installation, authorises it through Hypixel's official
device flow, maps the port on your router via UPnP, and hands you an address to
share. No manual port forwarding, no editing config files.

It also detects your installation and reports the game version and build, the
release channel, the launcher version, your saved-server count, and the last
signed-in profile.

## Status

| Feature | State |
| --- | --- |
| Install detection (Windows / macOS / Linux) | Working |
| Host a world | Working |
| Server authorisation (OAuth device flow) | Working |
| UPnP port mapping + connect address | Working |
| CGNAT detection | Working |
| Sign in with your Hytale account | Needs an OAuth `client_id` |
| Launch the game from HyPortal | Needs an OAuth `client_id` |

The last two are implemented end to end — PKCE, loopback redirect, token
exchange, session minting, and a direct spawn of `HytaleClient.exe` with the
session and identity JWTs it requires. They are gated on a `client_id` issued by
Hypixel Studios, because HyPortal will not reuse the official launcher's client
identity. See [docs/AUTH.md](docs/AUTH.md).

Only the Windows paths are verified against a real installation; the macOS and
Linux paths follow each platform's convention but are untested.

## Building

Requires the Rust toolchain (plus MSVC build tools on Windows). **No Node.js** —
the frontend is plain HTML, CSS, and JavaScript served straight from `ui/`, with
no bundler and no build step.

```sh
cd src-tauri
cargo run            # development
cargo tauri build    # installers, once tauri-cli is installed
```

To enable sign-in, put your OAuth client ID in `client_id.txt` next to the
executable, or set `HYPORTAL_CLIENT_ID`. The redirect URI to register is
`http://127.0.0.1:43110/callback`.

## Layout

```
ui/                        frontend, no build step
src-tauri/src/
  main.rs                  Tauri commands
  hytale.rs                install detection and game launch
  auth.rs                  OAuth 2.0 + PKCE sign-in
  server.rs                world hosting, server console
  net.rs                   UPnP mapping, reachability, CGNAT detection
tools/                     icon pipeline and diagnostics
docs/AUTH.md               how Hytale sign-in works, and HyPortal's approach
```

## Design rules

Deliberate constraints, not oversights:

1. **Never ship or download game files.** Only launch what is already installed.
2. **Never read `account.dat` or `.keys/`.** Profile details come from the
   launcher's plaintext log instead.
3. **Never reuse the official launcher's OAuth client identity.**
4. **Stay visibly unaffiliated**, and honour any takedown request immediately.

## Licence

[MIT](LICENSE).
