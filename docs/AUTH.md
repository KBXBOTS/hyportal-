# Authentication

## Current behaviour

HyPortal does not handle credentials. Pressing **Play** starts the official Hytale
launcher, which owns sign-in and starts the game. HyPortal's job today is
detection, presentation, and handoff.

This is deliberate. It keeps HyPortal clear of the two things the Hytale EULA
actually prohibits — distributing game files (§3.3) and circumventing technical
protection measures (§4.1) — while still being useful.

**HyPortal never reads `account.dat` or `.keys/*.key`.** Those hold the official
launcher's credential material. Profile name and UUID shown in the UI come from
`hytale-launcher.log`, which is plaintext and non-secret.

## How official sign-in works

Observed from the launcher's own log on a local install. This is standard
OAuth 2.0 for Native Apps (RFC 8252):

1. Launcher binds a loopback listener, e.g. `127.0.0.1:65424`
   ```
   msg="loopback server starting" addr=127.0.0.1:65424
   ```
2. System browser opens `oauth.accounts.hytale.com`; the user signs in there.
3. Provider redirects back to the loopback with an authorization code:
   ```
   msg="loopback result received" has_token=true error=<nil>
   ```
4. Tokens are persisted; refresh tokens are logged as "offline tokens":
   ```
   msg="refreshing offline tokens"
   msg="offline tokens refreshed" profiles=1
   ```
5. At launch, the launcher exchanges them for a short-lived game session:
   ```
   msg="requesting game session for authenticated launch" uuid=<profile-uuid>
   ```
6. It spawns the client, which validates the session against
   `https://sessions.hytale.com`:
   ```
   Auth config: mode=Authenticated, uuid=<uuid>, name=<name>
   Session token: iss=https://sessions.hytale.com, sub=<uuid>, scope=hytale:client
   Scheduling session refresh in 54m 57s
   ```

Session tokens are JWTs with `scope=hytale:client` and roughly a one-hour TTL,
refreshed about five minutes before expiry.

The related **server** flow is publicly documented by Hypixel Studios and uses
the same provider, including an OAuth device flow at
`https://oauth.accounts.hytale.com/oauth2/device/verify`, with `--session-token`
and `--identity-token` accepted on the server command line.

## The upgrade path

To sign in inside HyPortal, we need our **own `client_id`** issued by Hypixel
Studios for the `hytale:client` scope. With one, the flow is ordinary and needs
no reverse engineering:

1. Bind an ephemeral loopback port.
2. Open the system browser to the authorization endpoint with our `client_id`,
   a PKCE `code_challenge` (S256), and the loopback `redirect_uri`.
3. Receive the code on the loopback, exchange it for access + refresh tokens.
4. Mint a `hytale:client` session token from the session service.
5. Spawn `HytaleClient.exe` with that session, refreshing before expiry.

Store refresh tokens in the OS keychain (DPAPI / Keychain / libsecret), never in
a flat file.

### Confirmed endpoints

Cross-checked against HyPrism's public source, which implements the same flow:

| Purpose | URL |
| --- | --- |
| Authorize | `https://oauth.accounts.hytale.com/oauth2/auth` |
| Token | `https://oauth.accounts.hytale.com/oauth2/token` |
| Game session | `https://sessions.hytale.com/game-session/new` |
| Launcher data | `https://account-data.hytale.com/my-account/get-launcher-data` |

### How other launchers solve this

Surveyed in July 2026. There are exactly two approaches in the wild, and
HyPortal uses neither:

1. **Offline mode** — `--auth-mode offline` with fabricated UUIDs and tokens
   (HRS Launcher, HytaleSP, the various "F2P" launchers). This is piracy
   tooling. HRS's own README states that using it "results in the automatic
   termination of your license (Section 11.2)."
2. **Client impersonation** — sending `client_id = "hytale-launcher"`, the
   official launcher's own identity. Used by both HyPrism and Butter Launcher;
   the latter also sends `Authorization: Basic aHl0YWxlLWxhdW5jaGVyOg==`, which
   decodes to `hytale-launcher:` — the client ID with an empty secret,
   confirming it is a public client. Technically trivial for exactly that
   reason, but it means the app misrepresents itself to Hypixel's authorization
   server, and enforcement would fall on end users' accounts.

Notably, Butter Launcher resolves its client ID from `HYTALE_OAUTH_CLIENT_ID`
and only falls back to `hytale-launcher`. HyPortal uses the same seam
(`HYPORTAL_CLIENT_ID` / `client_id.txt`) but ships no fallback.

No third-party launcher appears to hold its own registered `client_id`. That
remains the only legitimate route, and it is why HyPortal delegates instead.

### What we will not do

- **Reuse the official launcher's `client_id`.** Public OAuth clients hold no
  secret, so this is technically trivial and ethically not — it presents HyPortal
  to Hypixel's auth server as their own application.
- **Extract tokens from `account.dat` / `.keys/`.** `env.dat` is encrypted and
  the `.key` files look like protection measures. That is §4.1 territory.

Until a `client_id` exists, delegation stands. The UI contract does not change
when we swap the implementation — `launch_game` keeps the same signature.
