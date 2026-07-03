<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="128" height="128" alt="Ember Logo">
</p>

<h1 align="center">Ember</h1>

<p align="center">
  <strong>The in-the-moment prompt refiner for any app.</strong><br>
  <em>Select text anywhere, press a shortcut, and watch it refine in place.</em>
</p>

<p align="center">
  <a href="https://github.com/duartelcunha/ember/releases/latest"><img src="https://img.shields.io/github/v/release/duartelcunha/ember?style=flat-square&color=orange" alt="Latest Release"></a>
  <a href="https://github.com/duartelcunha/ember/blob/main/LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square" alt="License"></a>
  <a href="https://tauri.app/"><img src="https://img.shields.io/badge/Built%20with-Tauri%202-yellow.svg?style=flat-square" alt="Tauri"></a>
</p>

---

Ember is a completely frictionless, native AI writing assistant that lives in your system tray. Select text anywhere, press the global shortcut, and Ember refines the selection in place. A beautiful UI element loads next to your cursor, the text is rewritten by a state-of-the-art LLM, and your selection is magically replaced. 

**No window switching. No copy-paste dance. Just magic.**

## ✨ Features

- ⚡ **Auto refine in place:** A global hotkey captures your selection, refines it, and pastes the result directly over the original text. Your original clipboard is restored automatically!
- 🛡️ **Resilient by design:** Primary provider is Google's Gemini, with Anthropic's Claude as an intelligent fallback. Fully handles rate-limits, context windows, and transient errors.
- 🔒 **BYOK (Bring Your Own Key):** Your API keys are strictly local. They live heavily encrypted in the Windows Credential Manager and are never sent anywhere but the provider. Privacy first!
- 🎭 **Custom Profiles:** Auto-detects contexts from `CLAUDE.md` or edit the system prompts directly in the beautiful settings panel.
- 💫 **Silky Smooth UI:** Micro-animations, dynamic glassmorphism, and a slick orb that elegantly follows your cursor.

## 🚀 Quick Start

1. Head over to the [Releases](https://github.com/duartelcunha/ember/releases/latest) page and download the latest `.exe` installer.
2. Launch Ember (it will live quietly in your system tray).
3. Click the Tray Icon and open **Settings** to add your Gemini or Claude API key.
4. Select any text in any app, and press `Ctrl+Shift+Space`.
5. Watch the magic happen!

## 🛠️ Stack

Built for maximum performance, minimal footprint, and stunning UI:
- **Core:** Tauri 2 (Rust shell)
- **Frontend:** React 19, Vite, Tailwind CSS 4, Framer Motion
- **Architecture:** The pure logic (refine pipeline, selection sequencing) lives in the `ember-core` crate and is fully unit-tested.

## 👨‍💻 Development

Want to build it from source or contribute?

```bash
# Install dependencies
npm install

# Run locally in dev mode
npm run tauri dev
```

The app runs in the system tray. Default shortcut: `Ctrl+Shift+Space`. Open Settings from the tray to tweak everything!

## 🧪 Testing

```bash
cargo test -p ember-core
```

## 📜 Versioning & Auto-Updates

- `package.json` is the single source of truth for versions.
- **Auto-Updates:** Built right in. When a new version is released, Ember will seamlessly prompt you to update to the newest features.
- We follow [Conventional Commits](https://www.conventionalcommits.org/).

## ⚖️ License & Copyright

Ember is dual-licensed under MIT/Apache-2.0. However, the Ember name and logo are trademarks.

---
<p align="center">Made with ❤️ for frictionless workflows.</p>
