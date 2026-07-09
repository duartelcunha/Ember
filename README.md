<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="112" height="112" alt="Ember">
</p>

<h1 align="center">Ember</h1>

<p align="center">
  <strong>Refine any prompt, in the moment, in any app.</strong><br>
  <em>Select text. Press a shortcut. Watch it sharpen in place.</em>
</p>

<p align="center">
  <a href="https://github.com/duartelcunha/Ember/releases/latest"><img src="https://img.shields.io/github/v/release/duartelcunha/Ember?style=flat-square&color=ff7a18&label=release" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/platform-Windows%20·%20macOS-2e2519?style=flat-square" alt="Platform">
  <a href="https://tauri.app/"><img src="https://img.shields.io/badge/built%20with-Tauri%202-ffcb47?style=flat-square" alt="Tauri 2"></a>
  <img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-4da3ff?style=flat-square" alt="License">
</p>

---

Ember lives in your system tray and gets out of the way. Highlight text in **any**
app, hit the global shortcut, and a small orb appears by your cursor while a
state-of-the-art model rewrites your selection. The refined text drops straight
back in place. Your clipboard is restored untouched.

**No window switching. No copy-paste dance. No tab you forgot to close.**

<br>

## Why Ember

|  |  |
|---|---|
| ⚡ **Refine in place** | A global hotkey captures your selection, refines it, and pastes the result over the original, then quietly restores your clipboard. Works in editors, browsers, chat apps, and terminals. |
| 🆓 **Free by default** | Runs on Google **Gemini** (generous free tier) as primary, with an **OpenAI-compatible** fallback defaulting to **OpenRouter** and a free reasoning model (DeepSeek R1). Point it at DeepSeek, Groq, or a local Ollama with one field. |
| 🛡️ **Resilient, not fragile** | A pure retry/fallback state machine handles rate-limits, truncation, content-policy, and outages. Fallbacks are pre-validated at startup, never guessed at the moment of failure. It degrades honestly instead of silently. |
| 🔒 **BYOK, strictly local** | Your API keys live in the OS credential vault (Windows Credential Manager / macOS Keychain), never in plain text, never anywhere but the provider. |
| 🎭 **Knows your project** | Optionally merges the `CLAUDE.md` / `AGENTS.md` / `GEMINI.md` of your focused project into the refine, with secret-shaped lines redacted. Off by default. |
| 💫 **Silky, deliberate UI** | Compositor-only animations tuned for 120fps, a cursor-following orb, glassmorphism, and a warm **Dark** or **Cream** theme. Respects your reduced-motion setting. |

<br>

## Quick start

1. Grab the latest installer from the [**Releases**](https://github.com/duartelcunha/Ember/releases/latest) page.
2. Launch Ember. It settles into your system tray.
3. Open **Settings** from the tray and paste a free [OpenRouter](https://openrouter.ai/keys) or [Gemini](https://aistudio.google.com/apikey) key.
4. Select text in any app and press `Ctrl+Shift+Space`.
5. Watch it refine.

> **Terminals are handled.** In Windows Terminal, PowerShell, and friends, Ember
> uses `Ctrl+Shift+C/V`, replaces the current input line instead of appending, and
> flattens the result to a single line so a stray newline never submits your command.

<br>

## The refine chain

Ember tries providers in priority order, keeping only the ones you've configured:

```
Gemini  →  OpenAI-compatible (OpenRouter)  →  Claude
primary        default fallback              optional third family
```

Transient errors retry with backoff on the same provider; only on exhaustion does
it fall to the next family. Auth and truncation fall over immediately (the other
family has a different key and different limits). Non-transient errors (a bad
payload, a content-policy refusal) propagate without masking. Every branch of this
lives in `ember-core` as a pure, network-free, unit-tested function.

<br>

## Stack

- **Shell:** Tauri 2 (Rust) — clipboard, input simulation, tray, windows.
- **Frontend:** React 19, Vite 7, Tailwind CSS 4, Motion.
- **Core:** the `ember-core` crate holds the refine pipeline, selection
  sequencing, provider wire-formats, and the resilience state machine, fully
  unit-tested with no I/O.

The split is deliberate: everything that can be reasoned about is pure and tested;
the shell is a thin layer of I/O around it.

<br>

## Development

```bash
npm install          # dependencies
npm run tauri dev    # run in dev (tray app + hot reload)
```

Default shortcut: `Ctrl+Shift+Space`. Everything is tweakable from Settings.

**Tests** (the whole workspace, matching CI):

```bash
cargo test --workspace
```

<br>

## Versioning & updates

- `package.json` is the single source of truth for the version.
- Releases are cut by [release-please](https://github.com/googleapis/release-please)
  from [Conventional Commits](https://www.conventionalcommits.org/); merging the
  standing release PR tags and publishes signed installers.
- **Auto-update** is built in: Ember checks the latest signed GitHub release and
  updates in place.

<br>

## License

Dual-licensed under **MIT** / **Apache-2.0**. The Ember name and logo are trademarks.

---
<p align="center"><sub>Built for frictionless writing. 🔥</sub></p>
