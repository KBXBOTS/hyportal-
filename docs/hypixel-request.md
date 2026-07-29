# Paste-ready: OAuth client_id request

Send this in the Hytale Discord (developer / API channel) or at
https://support.hytale.com. Replace `<YOUR NAME>` and send. Nothing else needed.

---

Hi — I'm building **HyPortal**, a small open-source community launcher for Hytale.
It focuses on making it easy to host a private world for a few friends.

It is not a distribution channel. It requires Hytale to be bought and installed
through the official launcher, and it never bundles, downloads, or redistributes
any game or server files — it only launches what's already installed.

At the moment HyPortal delegates sign-in to the official launcher, because I
didn't want to reuse a client identity that isn't mine or touch stored
credentials. That works, but users see two windows during startup.

Could I request an **OAuth 2.0 `client_id` scoped to `hytale:client`**? That
would let HyPortal run a standard PKCE authorization-code flow with a loopback
redirect — the same pattern as your documented server device flow, and what
RFC 8252 recommends for native apps.

The full source is public at https://github.com/KBXBOTS/hyportal- if you'd like
to check any of that before deciding.

Happy to accept any conditions: rate limits, a source review, required
non-affiliation notices, branding rules, or a registration you can revoke at any
time.

If issuing client credentials to third-party launchers isn't something you do,
no problem at all — I'd just appreciate knowing so I can stop looking for a
better approach and keep the delegated flow. And if there's any part of this
you'd rather I didn't build, tell me and I'll drop it.

Thanks,
<YOUR NAME>

---

## Notes

- No repo URL required — the offer to share source privately covers it.
- The closing paragraph matters: giving them an easy "no" and meaning it is what
  separates this from requests that get ignored.
- Expect a slow reply or none. Don't block on it. Hosting works without it, and
  so does Play.
