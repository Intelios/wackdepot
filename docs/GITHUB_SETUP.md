# GitHub integration

WackDepot talks directly to GitHub.com. End users authorize WackDepot in their
browser; they do not need GitHub CLI. Access and refresh tokens live in macOS
Keychain, separate from settings, drafts, and repositories.

## Register the application

The application owner must register **two OAuth applications** in GitHub's
Developer settings: `WackDepot Dev` and `WackDepot`. Use the project's public
homepage and this callback URL for each registration:

```
http://127.0.0.1/github/callback
```

GitHub permits a dynamically allocated port for loopback callbacks. WackDepot
binds only to `127.0.0.1`, validates a random state, and exchanges the code with
PKCE S256. Enable expiring user access tokens when available. Device flow is
not needed. The requested scopes are `repo`, `read:org`, `workflow`, and
`offline_access`.

For a local setup, run `target/debug/wackdepot --configure-github-oauth` after
building. It reads one JSON line with hidden terminal input and stores both
registrations as native app configuration under app support:

```json
{"development":{"client_id":"...","client_secret":"..."},"production":{"client_id":"...","client_secret":"..."}}
```

An opt-in debug owner setup form is also available with
`sh scripts/dev-bundle.sh WACKDEPOT_GITHUB_SETUP=1`, under Settings → Integrations.
This form is absent from normal launches and release builds.

Alternatively, provide the corresponding registration's client configuration
through the environment when building:

```
WACKDEPOT_GITHUB_CLIENT_ID=<registration-client-id>
WACKDEPOT_GITHUB_CLIENT_SECRET=<registration-client-secret>
```

Do not place secrets in command arguments or shell history. Do not commit credentials or access
tokens. GitHub's client secret is packaged native-client configuration, not a
confidential server credential; login security depends on PKCE and callback
validation. See [GitHub's OAuth documentation](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps).

Bundle scripts export the corresponding native client registration from
`~/Library/Application Support/WackDepot/oauth-clients/` into
`Contents/Resources/github-oauth.json`; release bundling fails if
the production registration is missing. Build environment configuration is also
supported. These resources contain application credentials only. Registering an app and
authorizing its repository access are owner actions; neither is silently done
by the build. A development build without configuration displays setup guidance
when Connect GitHub is clicked.

## Storage and Git credentials

- Production Keychain service: `com.wackdepot.app.github`.
- Development Keychain service: `com.wackdepot.app.dev.github`.
- Owner's native OAuth registrations: `oauth-clients/development.json` and
  `oauth-clients/production.json` under app support, with owner-only permissions.
  These contain native client configuration, never user sessions.
- Drafts: `~/Library/Application Support/WackDepot/github/drafts/`, with
  development drafts under `github-dev/drafts/`. Files are account/repository/PR
  scoped and created with owner-only permissions.
- Sessions are loaded once and held in memory. API requests do not read Keychain.
  Background credential access never opens a system prompt; denied restoration
  offers reconnect, and a connection whose save is denied stays in memory for
  the current session. Renewal is serialized with Git credential helpers.
- API caches are bounded and kept in memory; disconnect invalidates in-flight
  responses and clears them. Drafts remain associated with their original
  account.
- PR Git operations inherit SSH and existing credential helpers. A command-
  scoped HTTPS helper can supply WackDepot's Keychain credential for
  `github.com`; tokens never enter arguments, remote URLs, or logs. No global
  Git configuration is changed.
- Contributor review branches are marked in local branch configuration to
  prevent accidental publication. Checkout never resets existing branches.

## Validation

The service supports an injectable HTTP transport. Unit tests do not use real
credentials or GitHub repositories.

For an optional **read-only** live contract check, sign in to `gh` and run:

```
cargo run -p wackdepot-github --example smoke -- owner/repo
```

The separate `exercise` example performs writes and is restricted to a
dedicated repository whose name starts with `wackdepot-pr-test-`. Seed `main`
and a different `feature` branch before invoking it:

```
cargo run -p wackdepot-github --example exercise -- owner/wackdepot-pr-test-name
```

It creates a draft PR, edits and submits a review, exercises discussions and
viewed state, closes/reopens the PR, and merges with an expected head SHA.
The examples keep the developer's existing GitHub CLI credential in memory.

For native UI QA before OAuth registration, a **debug build only** supports
`WACKDEPOT_GITHUB_QA=1`. This explicitly uses the developer's existing `gh`
session in memory without touching Keychain. It has no effect in release
builds. Point `WACKDEPOT_OPEN` at a clone of the dedicated test repository.

```
sh scripts/dev-bundle.sh WACKDEPOT_GITHUB_QA=1 WACKDEPOT_OPEN=/path/to/test-clone
```

This QA mode validates the native workflow and live API contracts; it does not
replace checking browser OAuth authorization with the registered client.
