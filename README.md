<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="96" height="96" alt="Ember">
</p>

<h1 align="center">Ember</h1>

<p align="center">
  <strong>Refine any text, in the moment, in any desktop app.</strong><br>
  <em>Select text. Press a global shortcut. Watch it sharpen in place.</em>
</p>

<p align="center">
  <a href="https://github.com/duartelcunha/Ember/releases/latest"><img src="https://img.shields.io/github/v/release/duartelcunha/Ember?style=for-the-badge&color=fd8c3c&labelColor=1a0e03&label=release" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/Windows-2e2519?style=for-the-badge&logo=windows&logoColor=ffffff" alt="Windows">
  <img src="https://img.shields.io/badge/macOS-in%20progress-6f6353?style=for-the-badge&labelColor=2e2519&logo=apple&logoColor=ffffff" alt="macOS in progress">
  <a href="https://tauri.app/"><img src="https://img.shields.io/badge/Tauri%202-2e2519?style=for-the-badge&logo=tauri&logoColor=24C8DB" alt="Tauri 2"></a>
  <img src="https://img.shields.io/badge/Rust-2e2519?style=for-the-badge&logo=rust&logoColor=f46623" alt="Rust">
  <img src="https://img.shields.io/badge/React%2019-2e2519?style=for-the-badge&logo=react&logoColor=61DAFB" alt="React 19">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-fd8c3c?style=for-the-badge&labelColor=1a0e03" alt="License"></a>
</p>

---

<p align="center">
  <img src="docs/media/refine-slack.gif" width="100%" alt="Ember in action refining a Slack message in-place">
</p>

You know that clumsy message, that rambling prompt you keep editing, or that rough commit note? Select it, hit your shortcut, and Ember cleans it up directly where your cursor is.

No window switching. No copy-paste dance. No lost focus.

---

## Precision Modes

Ember adapts its transformation engine according to your intent:

<p align="center">
  <img src="docs/media/settings-refining-dark.png" width="100%" alt="Ember Refining Modes and Extended Thinking">
</p>

- **Fix:** Corrects spelling, grammar, accents, and punctuation while strictly preserving structure and tone.
- **Improve:** Tightens phrasing, eliminates wordiness, and structures run-on thoughts into crisp bullet points.
- **Rebuild:** Transforms raw notes into an engineer-grade prompt complete with **Role**, **Context**, **Constraints**, and **Output Format**.

---

## Projects & Knowledge Context

Switch project-specific tone, guidelines, and vocabulary on the fly without polluting prompts or leaking private keys:

<p align="center">
  <img src="docs/media/project-picker.gif" width="100%" alt="Ember Project Picker activated by global hotkey">
</p>

- **Instant Hotkey Switcher:** Press <kbd>Ctrl+Shift+P</kbd> anywhere on your system to open a lightweight, cursor-anchored switcher and navigate between projects using arrow keys.
- **Distilled Rules:** Automatically digests conventions from `CLAUDE.md`, `AGENTS.md`, or `.cursorrules` to retain domain jargon without prompt injection risks.
- **Custom Accents & Icons:** Dedicated colors and icons reflect active projects on the ambient orb and tray icon.

---

## Preview Gate & Direct Approval

Review changes before applying them with zero disruption to your flow:

<p align="center">
  <img src="docs/media/preview-gate.gif" width="100%" alt="Ember Preview Gate confirming changes before paste">
</p>

- **Optional Preview Gate:** Review transformed text in a floating bubble: press <kbd>Enter</kbd> to paste, or <kbd>Esc</kbd> to keep your original draft.
- **Smart Select-All Fallback:** In chat composers (ChatGPT, Slack, Codex), pressing your shortcut with nothing selected automatically captures the active field.

---

## Terminal & CLI Workflow

<p align="center">
  <img src="docs/media/refine-terminal-claude-code.gif" width="100%" alt="Ember refining a prompt in Claude Code terminal">
</p>

When working in terminals (Windows Terminal, PowerShell, CMD, Alacritty, WezTerm, Warp, or CLI tools like Claude Code):
1. **Accidental Execution Prevention:** Output is automatically flattened to single-line commands to eliminate dangerous multi-line paste execution in interactive shells.
2. **Terminal-Safe Shortcuts:** Uses synchronous input injection and terminal-aware modifier sequences (`Ctrl+Shift+C/V`).
3. **Selection Recovery:** Falls back to selection clipboard memory if synthetic copy is blocked.

---

## Native Dark & Cream Themes

Ember blends natively with both dark and light desktop setups with instant theme switching:

<p align="center">
  <img src="docs/media/theme-morph.gif" width="100%" alt="Ember Settings theme morph between Dark and Cream">
</p>

- **BYOK (Bring Your Own Key):** Direct links to create keys in 1 click for Gemini, Groq, OpenAI, OpenRouter, and Anthropic.
- **Interactive Hotkey Recorder:** Set custom shortcuts for Main refinement, direct Fix, direct Rebuild, and Project Picker.

---

## Security & Privacy Architecture

Ember is engineered from the ground up with strict defensive security principles:

| Area | Security Guarantee |
|---|---|
| **Secret Storage** | API keys and OAuth tokens are stored exclusively in the OS Credential Vault (**Windows Credential Manager** / macOS Keychain). Keys never cross the IPC bridge to frontend JavaScript. |
| **Prompt Boundary Isolation** | Input text and project context are wrapped with strict anti-injection delimiters and escaped (`[EMBER_INPUT]`, `[EMBER_PROJECT_SOURCE]`, `[EMBER_PROJECT_CONTEXT]`). |
| **Window & Focus Isolation** | Overlay and Picker windows run with `focus: false` and strict Content Security Policy (`default-src 'self'`). Keystrokes land strictly in the user's active application. |
| **Input Hook Hygiene** | Low-level keyboard hooks (`WH_KEYBOARD_LL`) intercept only the single approval key (`Enter`/`Esc`) during preview gates and pass all other system keystrokes through untouched. |
| **OS Sandboxing** | Browser links and URLs are opened directly via Win32 `ShellExecuteW`, bypassing `cmd.exe` execution layers. |

---

## The Refine Fallback Chain

```text
[ Primary Provider ]               [ Fallback Provider ]
 Google Gemini Free Tier   ───►   OpenAI-Compatible (Groq / OpenRouter / Local Ollama)
```

- **Resilient State Machine:** Transient network errors and rate limits (`429`) automatically trigger exponential backoff according to `Retry-After` headers.
- **Free-Tier First:** Runs out-of-the-box on Google **Gemini** (generous free tier) with automatic fallback to **Groq** (free OpenAI-compatible endpoint, ~14,000 requests/day).
- **Clean Degradation:** If all configured providers fail, your original text remains intact on the clipboard and an unobtrusive notification is displayed.

---

## Quick Start

1. Download the installer from [**Releases**](https://github.com/duartelcunha/Ember/releases/latest).
2. Launch Ember (it minimizes to your system tray).
3. Open **Settings** (<kbd>Tray Icon</kbd> → **Settings**) and paste a free API key:
   - [Google AI Studio (Gemini)](https://aistudio.google.com/apikey)
   - [Groq Cloud](https://console.groq.com/keys)
4. Highlight any text in any application and press your global shortcut (configured on first launch).

---

## Development

### Prerequisites
- Node.js (LTS) & npm
- Rust stable (`rustup default stable`)
- Tauri v2 prerequisites for Windows / macOS

### Build & Run
```bash
# Install frontend dependencies
npm install

# Run in development mode (with HMR)
npm run tauri dev

# Run full test suite (matching CI)
cargo test --workspace

# Typecheck and bundle production release
npm run build
```

---

## License

Distributed under the [MIT License](LICENSE).
