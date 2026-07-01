# Ember

In-the-moment prompt refiner for any app. Select text anywhere, press the global
shortcut, and Ember refines the selection in place: a small orb loads next to your
cursor, the text is rewritten by an LLM, and your selection is replaced. No window,
no copy-paste dance.

- **Auto refine in place.** Hotkey on a selection captures it (via clipboard), refines
  it, and pastes the result back over the selection. Your original clipboard is restored.
- **Resilient by design.** Gemini is primary, Claude is the fallback (different families
  fail for different reasons), with transient retry and provider fallback on exhaustion.
- **BYOK.** Your API keys live in the Windows Credential Manager, never in plain text.
- **Guided by your profile.** Auto-detected from your `CLAUDE.md`, or edited in Settings.

## Stack

Tauri 2 (Rust shell) + React 19 / Vite / Tailwind 4. The pure logic (refine pipeline,
selection sequencing) lives in the `ember-core` crate and is unit-tested without I/O.

## Develop

```bash
npm install
npm run tauri dev
```

The app runs in the system tray. Default shortcut: `Ctrl+Shift+Space`. Open Settings from
the tray to add a key and tune the profile.

## Test

```bash
cargo test -p ember-core
```

## Versioning & releases

`package.json` is the single source of truth for the version: `src-tauri/tauri.conf.json`
reads it directly, and the Cargo workspace mirrors it (`Cargo.toml`'s
`[workspace.package] version`, with every crate inheriting via `version.workspace = true`).
Run `npm run version:check` to verify the two are in sync (or `-- --write` to fix a manual
drift).

Commits follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`,
`fix:`, `chore:`, ...). [release-please](https://github.com/googleapis/release-please)
reads them to maintain a standing release PR that bumps the version, regenerates
`CHANGELOG.md`, and tags `vX.Y.Z` on merge.

## Auto-update

The app checks `https://github.com/<owner>/ember/releases/latest/download/latest.json`
(Settings -> About -> Check for updates) and verifies the update signature against the
public key baked into `tauri.conf.json`. Two workflows produce a release:

1. `.github/workflows/release-please.yml` opens/updates the release PR; merging it tags
   `vX.Y.Z` and publishes a GitHub Release with the changelog.
2. `.github/workflows/release.yml` (triggered by that tag) builds the signed NSIS
   installer and updater artifacts and uploads them to the same release.

The build needs two repo secrets: `TAURI_SIGNING_PRIVATE_KEY` (contents of the
`.key` file from `tauri signer generate`) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
Losing the private key means existing installs can no longer verify future updates.
